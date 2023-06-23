pub mod analyze;
pub mod code_gen;
pub mod resolve;
pub mod unsupported_module;

use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
    sync::Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use auto_hash_map::AutoSet;
use turbo_tasks::{
    emit, CollectiblesSource, RawVc, ReadRef, TransientInstance, TransientValue, TryJoinIterExt,
    ValueToString, Vc,
};
use turbo_tasks_fs::{FileContent, FileLine, FileLinesContent, FileSystemPath};
use turbo_tasks_hash::{DeterministicHash, Xxh3Hash64Hasher};

use crate::{
    asset::{Asset, AssetContent},
    source_pos::SourcePos,
};

#[turbo_tasks::value(shared)]
#[derive(PartialOrd, Ord, Copy, Clone, Hash, Debug, DeterministicHash)]
#[serde(rename_all = "camelCase")]
pub enum IssueSeverity {
    Bug,
    Fatal,
    Error,
    Warning,
    Hint,
    Note,
    Suggestion,
    Info,
}

impl IssueSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueSeverity::Bug => "bug",
            IssueSeverity::Fatal => "fatal",
            IssueSeverity::Error => "error",
            IssueSeverity::Warning => "warning",
            IssueSeverity::Hint => "hint",
            IssueSeverity::Note => "note",
            IssueSeverity::Suggestion => "suggestion",
            IssueSeverity::Info => "info",
        }
    }

    pub fn as_help_str(&self) -> &'static str {
        match self {
            IssueSeverity::Bug => "bug in implementation",
            IssueSeverity::Fatal => "unrecoverable problem",
            IssueSeverity::Error => "problem that cause a broken result",
            IssueSeverity::Warning => "problem should be addressed in short term",
            IssueSeverity::Hint => "idea for improvement",
            IssueSeverity::Note => "detail that is worth mentioning",
            IssueSeverity::Suggestion => "change proposal for improvement",
            IssueSeverity::Info => "detail that is worth telling",
        }
    }
}

impl Display for IssueSeverity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[turbo_tasks::value_trait]
pub trait Issue {
    /// Severity allows the user to filter out unimportant issues, with Bug
    /// being the highest priority and Info being the lowest.
    fn severity(self: Vc<Self>) -> Vc<IssueSeverity> {
        IssueSeverity::Error.into()
    }

    /// The file path that generated the issue, displayed to the user as message
    /// header.
    fn context(self: Vc<Self>) -> Vc<FileSystemPath>;

    /// A short identifier of the type of error (eg "parse", "analyze", or
    /// "evaluate") displayed to the user as part of the message header.
    fn category(self: Vc<Self>) -> Vc<String> {
        String::empty()
    }

    /// The issue title should be descriptive of the issue, but should be a
    /// single line. This is displayed to the user directly under the issue
    /// header.
    // TODO add Vc<StyledString>
    fn title(self: Vc<Self>) -> Vc<String>;

    /// A more verbose message of the issue, appropriate for providing multiline
    /// information of the issue.
    // TODO add Vc<StyledString>
    fn description(self: Vc<Self>) -> Vc<String>;

    /// Full details of the issue, appropriate for providing debug level
    /// information. Only displayed if the user explicitly asks for detailed
    /// messages (not to be confused with severity).
    fn detail(self: Vc<Self>) -> Vc<String> {
        String::empty()
    }

    /// A link to relevant documentation of the issue. Only displayed in console
    /// if the user explicitly asks for detailed messages.
    fn documentation_link(self: Vc<Self>) -> Vc<String> {
        String::empty()
    }

    /// The source location that caused the issue. Eg, for a parsing error it
    /// should point at the offending character. Displayed to the user alongside
    /// the title/description.
    fn source(self: Vc<Self>) -> Vc<OptionIssueSource> {
        OptionIssueSource::none()
    }

    fn sub_issues(self: Vc<Self>) -> Vc<Issues> {
        Vc::cell(Vec::new())
    }
}

#[turbo_tasks::value_trait]
trait IssueProcessingPath {
    fn shortest_path(
        self: Vc<Self>,
        issue: Vc<Box<dyn Issue>>,
    ) -> Vc<OptionIssueProcessingPathItems>;
}

#[turbo_tasks::value]
pub struct IssueProcessingPathItem {
    pub context: Option<Vc<FileSystemPath>>,
    pub description: Vc<String>,
}

#[turbo_tasks::value_impl]
impl ValueToString for IssueProcessingPathItem {
    #[turbo_tasks::function]
    async fn to_string(&self) -> Result<Vc<String>> {
        if let Some(context) = self.context {
            Ok(Vc::cell(format!(
                "{} ({})",
                context.to_string().await?,
                self.description.await?
            )))
        } else {
            Ok(self.description)
        }
    }
}

#[turbo_tasks::value_impl]
impl IssueProcessingPathItem {
    #[turbo_tasks::function]
    pub async fn into_plain(self: Vc<Self>) -> Result<Vc<PlainIssueProcessingPathItem>> {
        let this = self.await?;
        Ok(PlainIssueProcessingPathItem {
            context: if let Some(context) = this.context {
                Some(context.to_string().await?)
            } else {
                None
            },
            description: this.description.await?,
        }
        .cell())
    }
}

#[turbo_tasks::value(transparent)]
pub struct OptionIssueProcessingPathItems(Option<Vec<Vc<IssueProcessingPathItem>>>);

#[turbo_tasks::value_impl]
impl OptionIssueProcessingPathItems {
    #[turbo_tasks::function]
    pub fn none() -> Vc<Self> {
        Vc::cell(None)
    }

    #[turbo_tasks::function]
    pub async fn into_plain(self: Vc<Self>) -> Result<Vc<PlainIssueProcessingPath>> {
        Ok(Vc::cell(if let Some(items) = &*self.await? {
            Some(
                items
                    .iter()
                    .map(|item| item.into_plain())
                    .try_join()
                    .await?,
            )
        } else {
            None
        }))
    }
}

#[turbo_tasks::value]
struct RootIssueProcessingPath(Vc<Box<dyn Issue>>);

#[turbo_tasks::value_impl]
impl IssueProcessingPath for RootIssueProcessingPath {
    #[turbo_tasks::function]
    fn shortest_path(&self, issue: Vc<Box<dyn Issue>>) -> Vc<OptionIssueProcessingPathItems> {
        if self.0 == issue {
            Vc::cell(Some(Vec::new()))
        } else {
            Vc::cell(None)
        }
    }
}

#[turbo_tasks::value]
struct ItemIssueProcessingPath(
    Option<Vc<IssueProcessingPathItem>>,
    AutoSet<Vc<Box<dyn IssueProcessingPath>>>,
);

#[turbo_tasks::value_impl]
impl IssueProcessingPath for ItemIssueProcessingPath {
    /// Returns the shortest path from the root issue to the given issue.
    #[turbo_tasks::function]
    async fn shortest_path(
        &self,
        issue: Vc<Box<dyn Issue>>,
    ) -> Result<Vc<OptionIssueProcessingPathItems>> {
        assert!(!self.1.is_empty());
        let paths = self
            .1
            .iter()
            .map(|child| child.shortest_path(issue))
            .collect::<Vec<_>>();
        let paths = paths.iter().try_join().await?;
        let mut shortest: Option<&Vec<_>> = None;
        for path in paths.iter().filter_map(|p| p.as_ref()) {
            if let Some(old) = shortest {
                match old.len().cmp(&path.len()) {
                    Ordering::Greater => {
                        shortest = Some(path);
                    }
                    Ordering::Equal => {
                        let (mut a, mut b) = (old.iter(), path.iter());
                        while let (Some(a), Some(b)) = (a.next(), b.next()) {
                            let (a, b) = (a.to_string().await?, b.to_string().await?);
                            match String::cmp(&*a, &*b) {
                                Ordering::Less => break,
                                Ordering::Greater => {
                                    shortest = Some(path);
                                    break;
                                }
                                Ordering::Equal => {}
                            }
                        }
                    }
                    Ordering::Less => {}
                }
            } else {
                shortest = Some(path);
            }
        }
        Ok(Vc::cell(shortest.map(|path| {
            if let Some(item) = self.0 {
                std::iter::once(item).chain(path.iter().copied()).collect()
            } else {
                path.clone()
            }
        })))
    }
}

impl Issue {
    pub fn emit(self) {
        emit(self);
        emit(Vc::upcast(RootIssueProcessingPath::cell(
            RootIssueProcessingPath(self),
        )))
    }
}

impl Issue {
    #[allow(unused_variables, reason = "behind feature flag")]
    pub async fn attach_context<T: CollectiblesSource + Copy + Send>(
        context: impl Into<Option<Vc<FileSystemPath>>> + Send,
        description: impl Into<String> + Send,
        source: T,
    ) -> Result<T> {
        #[cfg(feature = "issue_path")]
        {
            let children = source.take_collectibles().await?;
            if !children.is_empty() {
                emit(
                    ItemIssueProcessingPath::cell(ItemIssueProcessingPath(
                        Some(IssueProcessingPathItem::cell(IssueProcessingPathItem {
                            context: context.into(),
                            description: Vc::cell(description.into()),
                        })),
                        children,
                    ))
                    .as_issue_processing_path(),
                );
            }
        }
        Ok(source)
    }

    #[allow(unused_variables, reason = "behind feature flag")]
    pub async fn attach_description<T: CollectiblesSource + Copy + Send>(
        description: impl Into<String> + Send,
        source: T,
    ) -> Result<T> {
        Self::attach_context(None, description, source).await
    }

    /// Returns all issues from `source` in a list with their associated
    /// processing path.
    pub async fn peek_issues_with_path<T: CollectiblesSource + Copy>(
        source: T,
    ) -> Result<Vc<CapturedIssues>> {
        Ok(CapturedIssues::cell(CapturedIssues {
            issues: source.peek_collectibles().strongly_consistent().await?,
            #[cfg(feature = "issue_path")]
            processing_path: ItemIssueProcessingPath::cell(ItemIssueProcessingPath(
                None,
                source.peek_collectibles().strongly_consistent().await?,
            )),
        }))
    }

    /// Returns all issues from `source` in a list with their associated
    /// processing path.
    ///
    /// This unemits the issues. They will not propagate up.
    pub async fn take_issues_with_path<T: CollectiblesSource + Copy>(
        source: T,
    ) -> Result<Vc<CapturedIssues>> {
        Ok(CapturedIssues::cell(CapturedIssues {
            issues: source.take_collectibles().strongly_consistent().await?,
            #[cfg(feature = "issue_path")]
            processing_path: ItemIssueProcessingPath::cell(ItemIssueProcessingPath(
                None,
                source.take_collectibles().strongly_consistent().await?,
            )),
        }))
    }
}

#[turbo_tasks::value(transparent)]
pub struct Issues(Vec<Vc<Box<dyn Issue>>>);

/// A list of issues captured with [`Issue::peek_issues_with_path`] and
/// [`Issue::take_issues_with_path`].
#[derive(Debug)]
#[turbo_tasks::value]
pub struct CapturedIssues {
    issues: AutoSet<Vc<Box<dyn Issue>>>,
    #[cfg(feature = "issue_path")]
    processing_path: Vc<ItemIssueProcessingPath>,
}

#[turbo_tasks::value_impl]
impl CapturedIssues {
    #[turbo_tasks::function]
    pub async fn is_empty(self: Vc<Self>) -> Result<Vc<bool>> {
        Ok(Vc::cell(self.await?.is_empty()))
    }
}

impl CapturedIssues {
    /// Returns true if there are no issues.
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    /// Returns the number of issues.
    pub fn len(&self) -> usize {
        self.issues.len()
    }

    /// Returns an iterator over the issues.
    pub fn iter(&self) -> impl Iterator<Item = Vc<Box<dyn Issue>>> + '_ {
        self.issues.iter().copied()
    }

    /// Returns an iterator over the issues with the shortest path from the root
    /// issue to each issue.
    pub fn iter_with_shortest_path(
        &self,
    ) -> impl Iterator<Item = (Vc<Box<dyn Issue>>, Vc<OptionIssueProcessingPathItems>)> + '_ {
        self.issues.iter().map(|issue| {
            #[cfg(feature = "issue_path")]
            let path = self.processing_path.shortest_path(*issue);
            #[cfg(not(feature = "issue_path"))]
            let path = OptionIssueProcessingPathItems::none();
            (*issue, path)
        })
    }

    pub async fn get_plain_issues(&self) -> Result<Vec<ReadRef<PlainIssue>>> {
        let mut list = self
            .issues
            .iter()
            .map(|&issue| async move {
                #[cfg(feature = "issue_path")]
                return issue
                    .into_plain(self.processing_path.shortest_path(issue))
                    .await;
                #[cfg(not(feature = "issue_path"))]
                return issue
                    .into_plain(OptionIssueProcessingPathItems::none())
                    .await;
            })
            .try_join()
            .await?;
        list.sort_by(|a, b| ReadRef::ptr_cmp(a, b));
        Ok(list)
    }
}

#[turbo_tasks::value]
#[derive(Clone)]
pub struct IssueSource {
    pub asset: Vc<Box<dyn Asset>>,
    pub start: SourcePos,
    pub end: SourcePos,
}

#[turbo_tasks::value_impl]
impl IssueSource {
    #[turbo_tasks::function]
    pub async fn from_byte_offset(
        asset: Vc<Box<dyn Asset>>,
        start: usize,
        end: usize,
    ) -> Result<Vc<Self>> {
        fn find_line_and_column(lines: &[FileLine], offset: usize) -> SourcePos {
            match lines.binary_search_by(|line| line.bytes_offset.cmp(&offset)) {
                Ok(i) => SourcePos { line: i, column: 0 },
                Err(i) => {
                    if i == 0 {
                        SourcePos {
                            line: 0,
                            column: offset,
                        }
                    } else {
                        SourcePos {
                            line: i - 1,
                            column: offset - lines[i - 1].bytes_offset,
                        }
                    }
                }
            }
        }
        Ok(Self::cell(
            if let FileLinesContent::Lines(lines) = &*asset.content().lines().await? {
                let start = find_line_and_column(lines.as_ref(), start);
                let end = find_line_and_column(lines.as_ref(), end);
                IssueSource { asset, start, end }
            } else {
                IssueSource {
                    asset,
                    start: SourcePos::default(),
                    end: SourcePos::max(),
                }
            },
        ))
    }
}

#[turbo_tasks::value(transparent)]
pub struct OptionIssueSource(Option<Vc<IssueSource>>);

#[turbo_tasks::value_impl]
impl OptionIssueSource {
    #[turbo_tasks::function]
    pub fn some(source: Vc<IssueSource>) -> Vc<Self> {
        Vc::cell(Some(source))
    }

    #[turbo_tasks::function]
    pub fn none() -> Vc<Self> {
        Vc::cell(None)
    }
}

#[turbo_tasks::value(serialization = "none")]
#[derive(Clone, Debug)]
pub struct PlainIssue {
    pub severity: IssueSeverity,
    pub context: String,
    pub category: String,

    pub title: String,
    pub description: String,
    pub detail: String,
    pub documentation_link: String,

    pub source: Option<ReadRef<PlainIssueSource>>,
    pub sub_issues: Vec<ReadRef<PlainIssue>>,
    pub processing_path: ReadRef<PlainIssueProcessingPath>,
}

fn hash_plain_issue(issue: &PlainIssue, hasher: &mut Xxh3Hash64Hasher, full: bool) {
    hasher.write_ref(&issue.severity);
    hasher.write_ref(&issue.context);
    hasher.write_ref(&issue.category);
    hasher.write_ref(&issue.title);
    hasher.write_ref(
        // Normalize syspaths from Windows. These appear in stack traces.
        &issue.description.replace('\\', "/"),
    );
    hasher.write_ref(&issue.detail);
    hasher.write_ref(&issue.documentation_link);

    if let Some(source) = &issue.source {
        hasher.write_value(1_u8);
        // I'm assuming we don't need to hash the contents. Not 100% correct, but
        // probably 99%.
        hasher.write_ref(&source.start);
        hasher.write_ref(&source.end);
    } else {
        hasher.write_value(0_u8);
    }

    if full {
        hasher.write_value(issue.sub_issues.len());
        for i in &issue.sub_issues {
            hash_plain_issue(i, hasher, full);
        }

        hasher.write_ref(&issue.processing_path);
    }
}

impl PlainIssue {
    /// We need deduplicate issues that can come from unique paths, but
    /// represent the same underlying problem. Eg, a parse error for a file
    /// that is compiled in both client and server contexts.
    ///
    /// Passing [full] will also hash any sub-issues and processing paths. While
    /// useful for generating exact matching hashes, it's possible for the
    /// same issue to pass from multiple processing paths, making for overly
    /// verbose logging.
    pub fn internal_hash(&self, full: bool) -> u64 {
        let mut hasher = Xxh3Hash64Hasher::new();
        hash_plain_issue(self, &mut hasher, full);
        hasher.finish()
    }
}

#[turbo_tasks::value_impl]
impl PlainIssue {
    /// We need deduplicate issues that can come from unique paths, but
    /// represent the same underlying problem. Eg, a parse error for a file
    /// that is compiled in both client and server contexts.
    ///
    /// Passing [full] will also hash any sub-issues and processing paths. While
    /// useful for generating exact matching hashes, it's possible for the
    /// same issue to pass from multiple processing paths, making for overly
    /// verbose logging.
    #[turbo_tasks::function]
    pub async fn internal_hash(self: Vc<Self>, full: bool) -> Result<Vc<u64>> {
        Ok(Vc::cell(self.await?.internal_hash(full)))
    }
}

#[turbo_tasks::value_impl]
impl Issue {
    #[turbo_tasks::function]
    pub async fn into_plain(
        self: Vc<Self>,
        processing_path: Vc<OptionIssueProcessingPathItems>,
    ) -> Result<Vc<PlainIssue>> {
        Ok(PlainIssue {
            severity: *self.severity().await?,
            context: self.context().to_string().await?.clone_value(),
            category: self.category().await?.clone_value(),
            title: self.title().await?.clone_value(),
            description: self.description().await?.clone_value(),
            detail: self.detail().await?.clone_value(),
            documentation_link: self.documentation_link().await?.clone_value(),
            source: {
                if let Some(s) = *self.source().await? {
                    Some(s.into_plain().await?)
                } else {
                    None
                }
            },
            sub_issues: self
                .sub_issues()
                .await?
                .iter()
                .map(|i| async move {
                    anyhow::Ok(i.into_plain(OptionIssueProcessingPathItems::none()).await?)
                })
                .try_join()
                .await?,
            processing_path: processing_path.into_plain().await?,
        }
        .cell())
    }
}

#[turbo_tasks::value(serialization = "none")]
#[derive(Clone, Debug)]
pub struct PlainIssueSource {
    pub asset: ReadRef<PlainAsset>,
    pub start: SourcePos,
    pub end: SourcePos,
}

#[turbo_tasks::value_impl]
impl IssueSource {
    #[turbo_tasks::function]
    pub async fn into_plain(self: Vc<Self>) -> Result<Vc<PlainIssueSource>> {
        let this = self.await?;
        Ok(PlainIssueSource {
            asset: PlainAsset::from_asset(this.asset).await?,
            start: this.start,
            end: this.end,
        }
        .cell())
    }
}

#[turbo_tasks::value(serialization = "none")]
#[derive(Clone, Debug)]
pub struct PlainAsset {
    pub ident: ReadRef<String>,
    #[turbo_tasks(debug_ignore)]
    pub content: ReadRef<FileContent>,
}

#[turbo_tasks::value_impl]
impl PlainAsset {
    #[turbo_tasks::function]
    pub async fn from_asset(asset: Vc<Box<dyn Asset>>) -> Result<Vc<PlainAsset>> {
        let asset_content = asset.content().await?;
        let content = match *asset_content {
            AssetContent::File(file_content) => file_content.await?,
            AssetContent::Redirect { .. } => ReadRef::new(Arc::new(FileContent::NotFound)),
        };

        Ok(PlainAsset {
            ident: asset.ident().to_string().await?,
            content,
        }
        .cell())
    }
}

#[turbo_tasks::value(transparent, serialization = "none")]
#[derive(Clone, Debug, DeterministicHash)]
pub struct PlainIssueProcessingPath(Option<Vec<ReadRef<PlainIssueProcessingPathItem>>>);

#[turbo_tasks::value(serialization = "none")]
#[derive(Clone, Debug, DeterministicHash)]
pub struct PlainIssueProcessingPathItem {
    pub context: Option<ReadRef<String>>,
    pub description: ReadRef<String>,
}

#[turbo_tasks::value_trait]
pub trait IssueReporter {
    fn report_issues(
        self: Vc<Self>,
        issues: TransientInstance<ReadRef<CapturedIssues>>,
        source: TransientValue<RawVc>,
    ) -> Vc<bool>;
}

#[async_trait]
pub trait IssueContextExt
where
    Self: Sized,
{
    async fn issue_context(
        self,
        context: impl Into<Option<Vc<FileSystemPath>>> + Send,
        description: impl Into<String> + Send,
    ) -> Result<Self>;
    async fn issue_description(self, description: impl Into<String> + Send) -> Result<Self>;
}

#[async_trait]
impl<T> IssueContextExt for T
where
    T: CollectiblesSource + Copy + Send,
{
    async fn issue_context(
        self,
        context: impl Into<Option<Vc<FileSystemPath>>> + Send,
        description: impl Into<String> + Send,
    ) -> Result<Self> {
        Issue::attach_context(context, description, self).await
    }
    async fn issue_description(self, description: impl Into<String> + Send) -> Result<Self> {
        Issue::attach_description(description, self).await
    }
}
