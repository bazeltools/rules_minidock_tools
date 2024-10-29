use std::sync::Arc;

use crate::registry::{
    http::{http_cli::RequestFailType, util::dump_body_to_string},
    DockerAuthenticationHelper,
};

use anyhow::Context;

use http::Uri;

use hyper::{Body, Client};

use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command};

use super::private_impl::{run_single_request, BearerConfig};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: Option<String>,
    pub access_token: Option<String>,
    pub expires_in: Option<u64>,
    pub issued_at: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct BasicAuthResponse {
    #[serde(rename = "Username")]
    username: String,

    #[serde(rename = "Secret")]
    secret: String,
}

pub async fn authenticate_request(
    auth_fail: &BearerConfig,
    inner_client: &Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
    docker_authorization_helpers: Arc<Vec<DockerAuthenticationHelper>>,
) -> Result<AuthResponse, RequestFailType> {
    let mut parts = auth_fail.realm.clone().into_parts();
    let new_query_items = if let Some(scope) = &auth_fail.scope {
        format!("service={}&scope={}", auth_fail.service, scope)
    } else {
        format!("service={}", auth_fail.service)
    };
    let existing_path_and_query = parts
        .path_and_query
        .as_ref()
        .map(|e| e.as_str())
        .unwrap_or("");
    let new_path_q = if existing_path_and_query.contains("?") {
        format!("{}&{}", existing_path_and_query, new_query_items)
    } else {
        format!("{}?{}", existing_path_and_query, new_query_items)
    };
    parts.path_and_query = Some(
        new_path_q
            .as_str()
            .try_into()
            .with_context(|| format!("Failed to parse path and query from {:?}", new_path_q))?,
    );
    let new_uri = Uri::from_parts(parts).with_context(|| {
        format!(
            "Failed to parse uri from installing new path and query of {}",
            new_path_q
        )
    })?;

    let matching_helper_opt: Option<&DockerAuthenticationHelper> = docker_authorization_helpers
        .iter()
        .find(|e| e.registry == auth_fail.service);

    let basic_auth_info = if let Some(matching_helper) = matching_helper_opt {
        let mut child = Command::new(&matching_helper.helper_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "Failed to start helper program at {:?}",
                    matching_helper.helper_path
                )
            })?;

        let mut child_stdin = child
            .stdin
            .take()
            .ok_or(anyhow::anyhow!("Failed to get stdin from running process"))?;
        child_stdin
            .write_all(format!("GET {}\n", &auth_fail.service).as_bytes())
            .await
            .with_context(|| {
                format!(
                    "Failed when trying to send domain into the stdin of the auth helper at {:?}",
                    &matching_helper.helper_path
                )
            })?;
        drop(child_stdin);

        let output = child.wait_with_output().await.with_context(|| {
            format!(
                "Failed waiting for output from subprocess calling {:?}",
                matching_helper.helper_path
            )
        })?;

        if output.status.success() {
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            Some(
                serde_json::from_str::<BasicAuthResponse>(&stdout_str).with_context(|| {
                    format!(
                        "Failed to parse output from {:?}, saw output:\n{}",
                        matching_helper.helper_path, stdout_str
                    )
                })?,
            )
        } else {
            return Err(RequestFailType::AnyhowError(anyhow::anyhow!(
                "Failed to run helper program at {:?}, got status code: {:?}, stderr: {:?}",
                matching_helper.helper_path,
                output.status,
                String::from_utf8(output.stderr)
            )));
        }
    } else {
        None
    };

    let mut response = run_single_request(
        Default::default(),
        &new_uri,
        basic_auth_info,
        |basic_auth_info, builder| async {
            use base64::prelude::*;

            let b2 = builder.method(http::Method::GET);
            let b3 = if let Some(ai) = basic_auth_info {
                b2.header(
                    "Authorization",
                    format!(
                        "Basic {}",
                        BASE64_STANDARD.encode(format!("{}:{}", ai.username, ai.secret).as_bytes())
                    ),
                )
            } else {
                b2
            };

            b3.body(Body::empty()).map_err(|e| e.into())
        },
        &inner_client,
    )
    .await?;

    if response.status().is_success() {
        let response_body = dump_body_to_string(&mut response).await?;
        let response_auth_info: AuthResponse =
            serde_json::from_str(&response_body).context("Decoding json body")?;
        return Ok(response_auth_info);
    } else {
        let try_response_body = dump_body_to_string(&mut response)
            .await
            .unwrap_or("".to_string());
        return Err(RequestFailType::AnyhowError(anyhow::anyhow!(
            "Failed to authenticate to {:?}, got status code: {:?}, body:\n{}",
            new_uri,
            response.status(),
            try_response_body
        )));
    }
}
