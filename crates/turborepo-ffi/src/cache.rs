use turbopath::AbsoluteSystemPathBuf;
use turborepo_api_client::APIClient;
use turborepo_cache::{http::HttpCache, signature_authentication::ArtifactSignatureAuthenticator};

use crate::{
    proto,
    proto::{NewHttpCacheRequest, RetrieveResponse},
    Buffer,
};

#[no_mangle]
#[tokio::main]
pub async extern "C" fn retrieve(buffer: Buffer) -> Buffer {
    let req: proto::RetrieveRequest = match buffer.into_proto() {
        Ok(req) => req,
        Err(err) => {
            let resp = proto::RetrieveResponse {
                response: Some(proto::retrieve_response::Response::Error(err.to_string())),
            };
            return resp.into();
        }
    };

    let http_cache_config = req.http_cache.expect("http_cache is required field");
    let http_cache = build_http_cache(http_cache_config)?;

    match http_cache
        .retrieve(
            &req.hash,
            &api_client_config.token,
            &api_client_config.team_id,
            api_client_config.team_slug.as_deref(),
            api_client_config.use_preflight,
        )
        .await
    {
        Ok((file_paths, duration)) => {
            let mut files = Vec::new();
            for path in file_paths {
                let path_str = match path.to_str() {
                    Ok(path_str) => path_str,
                    Err(err) => {
                        let resp = proto::RetrieveResponse {
                            response: Some(proto::retrieve_response::Response::Error(
                                err.to_string(),
                            )),
                        };
                        return resp.into();
                    }
                };

                files.push(path_str.to_string())
            }

            proto::RetrieveResponse {
                response: Some(proto::retrieve_response::Response::Files(
                    proto::RestoredFilesList { files, duration },
                )),
            }
            .into()
        }
        Err(err) => proto::RetrieveResponse {
            response: Some(proto::retrieve_response::Response::Error(err.to_string())),
        }
        .into(),
    }
}

#[no_mangle]
#[tokio::main]
pub async extern "C" fn put(buffer: Buffer) -> Buffer {
    let req: proto::PutArtifactRequest = match buffer.into_proto() {
        Ok(req) => req,
        Err(err) => {
            let resp = proto::RetrieveResponse {
                response: Some(proto::retrieve_response::Response::Error(err.to_string())),
            };
            return resp.into();
        }
    };

    let http_cache = build_http_cache(req.http_cache.expect("http cache is required"))?;
}

fn build_http_cache(req: NewHttpCacheRequest) -> Result<HttpCache, RetrieveResponse> {
    let api_client_config = req.api_client.expect("api_client is required field");

    let api_client = APIClient::new(
        &api_client_config.base_url,
        api_client_config.timeout,
        &api_client_config.version,
    )
    .expect("API client failed to build");

    let artifact_signature_authenticator = req
        .authenticator
        .map(|config| ArtifactSignatureAuthenticator::new(config.team_id));

    let Ok(repo_root) = AbsoluteSystemPathBuf::new(req.repo_root) else {
        let resp = proto::RetrieveResponse {
            response: Some(proto::retrieve_response::Response::Error(
                "repo_root is not absolute path".to_string(),
            )),
        };
        return resp.into();
    };

    Ok(HttpCache::new(
        api_client,
        artifact_signature_authenticator,
        repo_root,
    ))
}
