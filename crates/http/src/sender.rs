//! `HttpSender` implementation on reqwest.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use weft_connector_sdk::{ConnectorError, HttpRequest, HttpResponse, HttpSender, Method};

use crate::policy::SsrfPolicy;

/// Maximum response body kept in memory.
const MAX_BODY: usize = 10 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Resolves host names and drops addresses the policy forbids. Because the connection uses
/// only the filtered addresses, DNS rebinding cannot bypass the check; redirects re-resolve.
#[derive(Debug)]
struct GuardedResolver {
    policy: SsrfPolicy,
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let policy = self.policy;
        Box::pin(async move {
            let host = name.as_str().to_owned();
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let allowed: Vec<SocketAddr> = resolved.into_iter().filter(|a| policy.allows(a.ip())).collect();
            if allowed.is_empty() {
                let err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("destination `{host}` is not allowed"));
                return Err(Box::new(err) as Box<dyn std::error::Error + Send + Sync>);
            }
            let addrs: Addrs = Box::new(allowed.into_iter());
            Ok(addrs)
        })
    }
}

/// The production HTTP sender.
#[derive(Debug, Clone)]
pub struct ReqwestSender {
    client: reqwest::Client,
    policy: SsrfPolicy,
}

impl ReqwestSender {
    /// Builds the client. Fails only if the TLS backend cannot initialise.
    pub fn new(policy: SsrfPolicy) -> Result<Self, String> {
        let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if !destination_allowed(attempt.url(), policy) {
                attempt.error("redirect to a forbidden destination")
            } else {
                attempt.follow()
            }
        });
        let client = reqwest::Client::builder()
            .dns_resolver(Arc::new(GuardedResolver { policy }))
            .redirect(redirect_policy)
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Weft/0.1 (+integration platform)")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { client, policy })
    }
}

/// Scheme and literal-IP checks (hostnames are checked by the resolver).
fn destination_allowed(url: &url::Url, policy: SsrfPolicy) -> bool {
    if url.scheme() != "http" && url.scheme() != "https" {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) => policy.allows(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => policy.allows(IpAddr::V6(ip)),
        Some(url::Host::Domain(d)) => policy.allows_private() || !d.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

fn to_reqwest(method: Method) -> reqwest::Method {
    match method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Patch => reqwest::Method::PATCH,
        Method::Delete => reqwest::Method::DELETE,
    }
}

fn map_error(err: &reqwest::Error) -> ConnectorError {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = source {
        if let Some(io) = e.downcast_ref::<std::io::Error>()
            && io.kind() == std::io::ErrorKind::PermissionDenied
        {
            return ConnectorError::invalid(format!("blocked: {io}"));
        }
        source = e.source();
    }
    if err.is_timeout() {
        ConnectorError::transient("the app did not respond in time")
    } else if err.is_redirect() {
        ConnectorError::invalid(format!("redirect refused: {err}"))
    } else {
        ConnectorError::transient(format!("network error: {err}"))
    }
}

#[async_trait]
impl HttpSender for ReqwestSender {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ConnectorError> {
        let url = url::Url::parse(&request.url).map_err(|e| ConnectorError::invalid_field("url", format!("invalid URL: {e}")))?;
        if !destination_allowed(&url, self.policy) {
            return Err(ConnectorError::invalid_field("url", format!("destination not allowed: {url}")));
        }
        let mut builder = self.client.request(to_reqwest(request.method), url).timeout(request.timeout.unwrap_or(DEFAULT_TIMEOUT));
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let mut response = builder.send().await.map_err(|e| map_error(&e))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_owned(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
            .collect();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| map_error(&e))? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err(ConnectorError::permanent("response body exceeds 10 MB"));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpResponse { status, headers, body })
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

    use super::*;

    #[tokio::test]
    async fn sends_requests_when_private_networks_are_allowed() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/hook")).respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"ok": true}))).mount(&server).await;
        let sender = ReqwestSender::new(SsrfPolicy::allow_private()).unwrap();
        let resp = sender.send(HttpRequest::post(format!("{}/hook", server.uri())).json(&serde_json::json!({"a": 1}))).await.unwrap();
        assert_eq!(resp.status, 201);
        assert_eq!(resp.json().unwrap(), serde_json::json!({"ok": true}));
    }

    #[tokio::test]
    async fn blocks_private_destinations_by_default() {
        let sender = ReqwestSender::new(SsrfPolicy::public_only()).unwrap();
        for url in ["http://127.0.0.1:9/x", "http://169.254.169.254/latest/meta-data", "http://localhost:9/", "file:///etc/passwd", "http://[::1]:9/"] {
            let err = sender.send(HttpRequest::get(url)).await.unwrap_err();
            assert!(matches!(err, ConnectorError::InvalidInput { .. }), "{url}: {err:?}");
        }
    }
}
