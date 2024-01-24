use crate::registry::http::util::dump_body_to_string;

use anyhow::Context;

use http::Uri;

use hyper::{Body, Client};

use serde::{Deserialize, Serialize};

use super::private_impl::{run_single_request, BearerConfig};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: Option<String>,
    pub access_token: Option<String>,
    pub expires_in: Option<u64>,
    pub issued_at: Option<String>,
}

pub async fn authenticate_request(
    auth_fail: &BearerConfig,
    inner_client: &Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
) -> anyhow::Result<AuthResponse> {
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
    let basic_auth_info: Option<String> = None;
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
                    format!("Basic {}", BASE64_STANDARD.encode(ai)),
                )
            } else {
                b2
            };

            b3.body(Body::empty()).map_err(|e| e.into())
        },
        &inner_client,
    )
    .await
    .with_context(|| {
        format!(
            "Failed to run new request to try authenticate to {:?}",
            new_uri
        )
    })?;

    if response.status().is_success() {
        let response_body = dump_body_to_string(&mut response).await?;
        let response_auth_info: AuthResponse = serde_json::from_str(&response_body)?;
        return Ok(response_auth_info);
    } else {
        let try_response_body = dump_body_to_string(&mut response)
            .await
            .unwrap_or("".to_string());
        anyhow::bail!(
            "Failed to authenticate to {:?}, got status code: {:?}, body:\n{}",
            new_uri,
            response.status(),
            try_response_body
        );
    }
}
