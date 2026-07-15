use std::sync::Arc;

use anyhow::Context;
use http::Uri;
use http::{Response, StatusCode};

use hyper::{Body, Client};
use regex::Regex;

use super::authentication_flow::AuthResponse;

#[derive(Debug, Clone)]
pub struct BearerConfig {
    pub realm: Uri,
    pub service: String,
    pub scope: Option<String>,
}
impl std::fmt::Display for BearerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ realm: {}, service: {}, scope: {} }}",
            self.realm,
            self.service,
            self.scope.as_ref().map(|e| e.as_str()).unwrap_or("")
        )
    }
}

impl BearerConfig {
    pub fn from_auth_header(auth_header: &str) -> anyhow::Result<Self> {
        let mut realm = None;
        let mut scope = None;
        let mut service = None;

        let auth_header = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| anyhow::anyhow!("Invalid auth header"))?;

        // the csv thats used here the csv parsers i saw don't like
        // of the shape key="value",key="value3,e",y="value"
        //
        // Values aren't always quoted (e.g. gcr.io sends `service=gcr.io` unquoted
        // while quoting `realm`), so we match each `key=value` pair directly instead
        // of splitting into a flat token list on quote/comma boundaries and assuming
        // every pair produces exactly two tokens.
        let pair_regex = Regex::new(r#"([a-zA-Z_][a-zA-Z0-9_]*)=("[^"]*"|[^,]*)"#).unwrap();

        for cap in pair_regex.captures_iter(auth_header) {
            let key = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let value = cap
                .get(2)
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim_matches('"');
            match key {
                "realm" => {
                    realm = Some(
                        value
                            .parse()
                            .with_context(|| format!("Failed to parse realm from {:?}", value))?,
                    )
                }
                "service" => service = Some(value.to_string()),
                "scope" => scope = Some(value.to_string()),
                _ => (),
            }
        }

        match (realm, service) {
            (Some(realm), Some(service)) => Ok(Self {
                realm,
                service,
                scope,
            }),
            _ => Err(anyhow::anyhow!("Invalid auth header")),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RequestFailType {
    #[error("Failed to connect: '{0}'")]
    ConnectError(hyper::Error),
    #[error("Generic hyper error: '{0}'")]
    HyperError(hyper::Error),
    #[error("Internal error: '{0:?}'")]
    AnyhowError(anyhow::Error),
    #[error("Auth failed: '{1}'")]
    AuthFailure(Response<Body>, BearerConfig),
    #[error("Got a redirection code: '{0}'")]
    Redirection(String),
    #[error("Server error (retryable): status {0}")]
    ServerError(StatusCode, Response<Body>),
}
impl From<anyhow::Error> for RequestFailType {
    fn from(e: anyhow::Error) -> Self {
        RequestFailType::AnyhowError(e)
    }
}
pub async fn run_single_request<F, Fut, B>(
    auth_info: Arc<tokio::sync::Mutex<Option<AuthResponse>>>,
    uri: &Uri,
    context: B,
    complete_uri: F,
    inner_client: &Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
) -> Result<Response<Body>, RequestFailType>
where
    F: Fn(B, http::request::Builder) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<http::request::Request<Body>>>,
    B: Send + 'static + Sync,
{
    let req_builder = http::request::Builder::default().uri(uri);

    let li = auth_info.lock().await;
    let auth_token = li
        .as_ref()
        .and_then(|e| e.token.as_ref().or(e.access_token.as_ref()).cloned());
    drop(li);
    let req_builder = if let Some(token) = auth_token {
        req_builder.header(http::header::AUTHORIZATION, format!("Bearer {}", token))
    } else {
        req_builder
    };
    let request = complete_uri(context, req_builder).await?;

    let r: Response<Body> = match inner_client.request(request).await {
        Err(e) => {
            if e.is_connect() {
                return Err(RequestFailType::ConnectError(e));
            } else {
                return Err(RequestFailType::HyperError(e));
            }
        }
        Ok(r) => {
            if r.status() == StatusCode::UNAUTHORIZED {
                if let Some(auth_header) = r
                    .headers()
                    .get("WWW-Authenticate")
                    .map(|e| e.to_str().ok())
                    .flatten()
                {
                    let b = BearerConfig::from_auth_header(auth_header).with_context(|| {
                        format!(
                            "unable to parse auth header when issuing request, got header '{}'",
                            auth_header
                        )
                    })?;
                    return Err(RequestFailType::AuthFailure(r, b));
                }
            }
            if r.status().is_redirection() {
                if let Some(location_header) = r.headers().get(http::header::LOCATION) {
                    let location_str = location_header.to_str().with_context(|| {
                        format!("Unable to parse redirection header {:?}", location_header)
                    })?;
                    return Err(RequestFailType::Redirection(location_str.to_string()));
                }
            }
            // Handle server errors (502, 503, 504) as retryable conditions
            if r.status() == StatusCode::BAD_GATEWAY
                || r.status() == StatusCode::SERVICE_UNAVAILABLE
                || r.status() == StatusCode::GATEWAY_TIMEOUT
            {
                return Err(RequestFailType::ServerError(r.status(), r));
            }
            r
        }
    };
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decode_auth_header() {
        let header = "Bearer realm=\"https://auth.docker.io/token\",service=\"registry.docker.io\"";

        let hdr = BearerConfig::from_auth_header(&header).expect("Should be able to decode header");
        assert_eq!(
            hdr.realm,
            "https://auth.docker.io/token".parse::<Uri>().unwrap()
        );
        assert_eq!(hdr.service, "registry.docker.io");
        assert_eq!(hdr.service, "registry.docker.io");
    }

    #[test]
    fn test_decode_auth_header_gcr_unquoted_service() {
        // gcr.io returns `service` unquoted, unlike docker.io above which quotes it.
        let header = "Bearer realm=\"https://gcr.io/v2/token\",service=gcr.io";

        let hdr = BearerConfig::from_auth_header(header).expect("Should be able to decode header");
        assert_eq!(hdr.realm, "https://gcr.io/v2/token".parse::<Uri>().unwrap());
        assert_eq!(hdr.service, "gcr.io");
    }

    #[test]
    fn test_decode_auth_header_quoted_scope_with_comma() {
        // scope values can themselves contain commas (e.g. multiple actions on one
        // repo), which is exactly why the original parser tried to be comma-safe.
        let header = "Bearer realm=\"https://auth.docker.io/token\",service=\"registry.docker.io\",scope=\"repository:samalba/my-app:pull,push\"";

        let hdr = BearerConfig::from_auth_header(header).expect("Should be able to decode header");
        assert_eq!(
            hdr.realm,
            "https://auth.docker.io/token".parse::<Uri>().unwrap()
        );
        assert_eq!(hdr.service, "registry.docker.io");
        assert_eq!(hdr.scope, Some("repository:samalba/my-app:pull,push".to_string()));
    }

    #[test]
    fn test_decode_auth_header_space_after_comma() {
        // Azure Container Registry (and others) put a space after the comma, e.g.
        // `realm="...", service="..."` rather than docker.io/gcr.io's `realm="...",service="..."`.
        let header = "Bearer realm=\"https://x.azurecr.io/oauth2/token\", service=\"x.azurecr.io\"";

        let hdr = BearerConfig::from_auth_header(header).expect("Should be able to decode header");
        assert_eq!(
            hdr.realm,
            "https://x.azurecr.io/oauth2/token".parse::<Uri>().unwrap()
        );
        assert_eq!(hdr.service, "x.azurecr.io");
    }

    #[test]
    fn test_decode_auth_header_fully_unquoted() {
        // Neither value quoted at all.
        let header = "Bearer realm=https://example.com/token,service=example.com";

        let hdr = BearerConfig::from_auth_header(header).expect("Should be able to decode header");
        assert_eq!(hdr.realm, "https://example.com/token".parse::<Uri>().unwrap());
        assert_eq!(hdr.service, "example.com");
    }

    #[test]
    fn test_decode_auth_header_fields_out_of_order() {
        // Field order isn't guaranteed by the spec.
        let header = "Bearer service=registry.docker.io,realm=\"https://auth.docker.io/token\"";

        let hdr = BearerConfig::from_auth_header(header).expect("Should be able to decode header");
        assert_eq!(
            hdr.realm,
            "https://auth.docker.io/token".parse::<Uri>().unwrap()
        );
        assert_eq!(hdr.service, "registry.docker.io");
    }

    #[test]
    fn test_decode_auth_header_with_error_param() {
        // Registries often add an `error` param (RFC 6750 sec 3) alongside realm/service
        // when returning a 401, e.g. after an expired token. It's not a field we track,
        // but it must not break parsing of the other params.
        let header = "Bearer realm=\"https://auth.docker.io/token\",service=\"registry.docker.io\",error=\"invalid_token\"";

        let hdr = BearerConfig::from_auth_header(header).expect("Should be able to decode header");
        assert_eq!(
            hdr.realm,
            "https://auth.docker.io/token".parse::<Uri>().unwrap()
        );
        assert_eq!(hdr.service, "registry.docker.io");
    }

    #[test]
    fn test_decode_auth_header_missing_service_errors() {
        let header = "Bearer realm=\"https://auth.docker.io/token\"";
        assert!(BearerConfig::from_auth_header(header).is_err());
    }

    #[test]
    fn test_decode_auth_header_missing_realm_errors() {
        let header = "Bearer service=\"registry.docker.io\"";
        assert!(BearerConfig::from_auth_header(header).is_err());
    }

    #[test]
    fn test_decode_auth_header_not_bearer_errors() {
        let header = "Basic realm=\"https://auth.docker.io/token\"";
        assert!(BearerConfig::from_auth_header(header).is_err());
    }
}
