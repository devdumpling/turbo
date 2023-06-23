use std::io::Write;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use indoc::writedoc;
use serde::Serialize;
use turbo_tasks::{IntoTraitRef, TryJoinIterExt, Vc};
use turbo_tasks_fs::File;
use turbopack_core::{
    asset::{Asset, AssetContent},
    chunk::ChunkingContext,
    code_builder::{Code, CodeBuilder},
    version::{
        MergeableVersionedContent, Update, Version, VersionedContent, VersionedContentMerger,
        VersionedContents,
    },
};
use turbopack_ecmascript::utils::StringifyJs;

use super::{
    asset::{EcmascriptDevChunkList, EcmascriptDevChunkListSource},
    update::update_chunk_list,
    version::EcmascriptDevChunkListVersion,
};

/// Contents of an [`EcmascriptDevChunkList`].
#[turbo_tasks::value]
pub(super) struct EcmascriptDevChunkListContent {
    chunk_list_path: String,
    pub(super) chunks_contents: IndexMap<String, Vc<Box<dyn VersionedContent>>>,
    source: EcmascriptDevChunkListSource,
}

#[turbo_tasks::value_impl]
impl EcmascriptDevChunkListContent {
    /// Creates a new [`EcmascriptDevChunkListContent`].
    #[turbo_tasks::function]
    pub async fn new(chunk_list: Vc<EcmascriptDevChunkList>) -> Result<Vc<Self>> {
        let chunk_list_ref = chunk_list.await?;
        let output_root = chunk_list_ref.chunking_context.output_root().await?;
        Ok(EcmascriptDevChunkListContent {
            chunk_list_path: output_root
                .get_path_to(&*chunk_list.ident().path().await?)
                .context("chunk list path not in output root")?
                .to_string(),
            chunks_contents: chunk_list_ref
                .chunks
                .await?
                .iter()
                .map(|chunk| {
                    let output_root = output_root.clone();
                    async move {
                        Ok((
                            output_root
                                .get_path_to(&*chunk.ident().path().await?)
                                .map(|path| path.to_string()),
                            chunk.versioned_content(),
                        ))
                    }
                })
                .try_join()
                .await?
                .into_iter()
                .filter_map(|(path, content)| path.map(|path| (path, content)))
                .collect(),
            source: chunk_list_ref.source,
        }
        .cell())
    }

    /// Computes the version of this content.
    #[turbo_tasks::function]
    pub async fn version(self: Vc<Self>) -> Result<Vc<EcmascriptDevChunkListVersion>> {
        let this = self.await?;

        let mut by_merger = IndexMap::<_, Vec<_>>::new();
        let mut by_path = IndexMap::<_, _>::new();

        for (chunk_path, chunk_content) in &this.chunks_contents {
            if let Some(mergeable) =
                Vc::try_resolve_sidecast::<Box<dyn MergeableVersionedContent>>(chunk_content)
                    .await?
            {
                let merger = mergeable.get_merger().resolve().await?;
                by_merger.entry(merger).or_default().push(*chunk_content);
            } else {
                by_path.insert(
                    chunk_path.clone(),
                    chunk_content.version().into_trait_ref().await?,
                );
            }
        }

        let by_merger = by_merger
            .into_iter()
            .map(|(merger, contents)| {
                let merger = merger;
                async move {
                    Ok((
                        merger,
                        merger
                            .merge(Vc::cell(contents))
                            .version()
                            .into_trait_ref()
                            .await?,
                    ))
                }
            })
            .try_join()
            .await?
            .into_iter()
            .collect();

        Ok(EcmascriptDevChunkListVersion { by_path, by_merger }.cell())
    }

    #[turbo_tasks::function]
    pub(super) async fn code(self: Vc<Self>) -> Result<Vc<Code>> {
        let this = self.await?;

        let params = EcmascriptDevChunkListParams {
            path: &this.chunk_list_path,
            chunks: this.chunks_contents.keys().map(|s| s.as_str()).collect(),
            source: this.source,
        };

        let mut code = CodeBuilder::default();

        // When loaded, JS chunks must register themselves with the `TURBOPACK` global
        // variable. Similarly, we register the chunk list with the
        // `TURBOPACK_CHUNK_LISTS` global variable.
        writedoc!(
            code,
            r#"
                (globalThis.TURBOPACK = globalThis.TURBOPACK || []).push([
                    {},
                    {{}},
                ]);
                (globalThis.TURBOPACK_CHUNK_LISTS = globalThis.TURBOPACK_CHUNK_LISTS || []).push({:#});
            "#,
            StringifyJs(&this.chunk_list_path),
            StringifyJs(&params),
        )?;

        Ok(Code::cell(code.build()))
    }
}

#[turbo_tasks::value_impl]
impl VersionedContent for EcmascriptDevChunkListContent {
    #[turbo_tasks::function]
    async fn content(self: Vc<Self>) -> Result<Vc<AssetContent>> {
        let code = self.code().await?;
        Ok(File::from(code.source_code().clone()).into())
    }

    #[turbo_tasks::function]
    fn version(self: Vc<Self>) -> Vc<Box<dyn Version>> {
        self.version().into()
    }

    #[turbo_tasks::function]
    fn update(self: Vc<Self>, from_version: Vc<Box<dyn Version>>) -> Vc<Update> {
        update_chunk_list(self, from_version)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EcmascriptDevChunkListParams<'a> {
    /// Path to the chunk list to register.
    path: &'a str,
    /// All chunks that belong to the chunk list.
    chunks: Vec<&'a str>,
    /// Where this chunk list is from.
    source: EcmascriptDevChunkListSource,
}
