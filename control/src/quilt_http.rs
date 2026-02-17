use anyhow::{Context, Result};
use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Request, Response, StatusCode, Uri};
use futures::StreamExt;
use reqwest::Url;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;

#[derive(Clone)]
pub enum QuiltAuth {
    ApiKey(Arc<str>),
    BearerToken(Arc<str>),
}

#[derive(Clone)]
pub struct QuiltHttpClient {
    base_url: Url,
    auth: Option<QuiltAuth>,
    client: reqwest::Client,
}

impl QuiltHttpClient {
    pub fn new(base_url: &str, auth: Option<QuiltAuth>) -> Result<Self> {
        let base_url = Url::parse(base_url).context("Invalid QUILT_API_BASE_URL")?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("Failed to build Quilt HTTP client")?;

        Ok(Self {
            base_url,
            auth,
            client,
        })
    }

    pub async fn proxy(&self, req: Request<Body>) -> Result<Response<Body>> {
        let (parts, body) = req.into_parts();
        let method = parts.method;
        let uri = parts.uri;

        let url = self.upstream_url(&uri)?;
        debug!("Proxying {} {} -> {}", method, uri, url);

        // Stream request body to upstream. This keeps large uploads workable.
        let body_stream = body
            .into_data_stream()
            .map(|chunk| chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));
        let upstream_body = reqwest::Body::wrap_stream(body_stream);

        let mut upstream_req = self.client.request(method, url);
        upstream_req = upstream_req.body(upstream_body);

        // Headers: forward most, strip hop-by-hop, and inject auth + request id.
        let mut headers = parts.headers;
        sanitize_hop_by_hop(&mut headers);
        ensure_request_id(&mut headers);

        if let Some(auth) = &self.auth {
            match auth {
                QuiltAuth::ApiKey(k) => {
                    headers.insert(
                        HeaderName::from_static("x-api-key"),
                        HeaderValue::from_str(k).context("Invalid QUILT_API_KEY value")?,
                    );
                }
                QuiltAuth::BearerToken(t) => {
                    let v = format!("Bearer {}", t);
                    headers.insert(
                        HeaderName::from_static("authorization"),
                        HeaderValue::from_str(&v).context("Invalid QUILT_JWT value")?,
                    );
                }
            }
        }

        for (k, v) in headers.iter() {
            upstream_req = upstream_req.header(k, v);
        }

        let upstream_resp = upstream_req
            .send()
            .await
            .context("Upstream Quilt request failed")?;

        let status = upstream_resp.status();
        let mut resp_headers = axum::http::HeaderMap::new();
        for (k, v) in upstream_resp.headers().iter() {
            resp_headers.insert(k.clone(), v.clone());
        }
        sanitize_hop_by_hop(&mut resp_headers);

        let stream = upstream_resp
            .bytes_stream()
            .map(|r| r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

        let mut resp = Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY));

        {
            let headers_mut = resp.headers_mut().expect("builder headers");
            for (k, v) in resp_headers.iter() {
                headers_mut.insert(k.clone(), v.clone());
            }
        }

        Ok(resp.body(Body::from_stream(stream)).unwrap())
    }

    fn upstream_url(&self, uri: &Uri) -> Result<Url> {
        let pq = uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or(uri.path());

        // Url::join treats absolute paths as path-rooted on the base, which is what we want.
        let joined = self.base_url.join(pq).with_context(|| {
            format!("Failed to join base_url={} with path={}", self.base_url, pq)
        })?;
        Ok(joined)
    }
}

fn sanitize_hop_by_hop(headers: &mut HeaderMap) {
    // RFC 7230 hop-by-hop headers should not be forwarded by proxies.
    // We also strip content-length since streaming bodies may change framing.
    let hop_by_hop: HashSet<&'static str> = [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
        "transfer-encoding",
        "upgrade",
        "content-length",
    ]
    .into_iter()
    .collect();

    for name in hop_by_hop {
        headers.remove(name);
    }
}

fn ensure_request_id(headers: &mut HeaderMap) {
    // Quilt backend will echo X-Request-ID if present; otherwise it generates one.
    // We ensure one is present for traceability when requests originate from this control plane.
    let name = HeaderName::from_static("x-request-id");
    if headers.contains_key(&name) {
        return;
    }

    match HeaderValue::from_str(&Uuid::new_v4().to_string()) {
        Ok(v) => {
            headers.insert(name, v);
        }
        Err(e) => {
            warn!("Failed to set x-request-id header: {}", e);
        }
    }
}
