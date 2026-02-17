use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, StatusCode, Url};
use std::io::Write;
use std::time::Duration;
use tracing::debug;
use uuid::Uuid;

use crate::error::ErrorResponse;

#[derive(Clone, Debug)]
pub enum TenantAuth {
    Jwt(String),
    ApiKey(String),
}

#[derive(Clone)]
pub struct Client {
    base_url: Url,
    http: reqwest::Client,
    tenant_auth: Option<TenantAuth>,
    user_agent: String,
    retries: u32,
}

impl Client {
    pub fn new(
        base_url: &str,
        tenant_auth: Option<TenantAuth>,
        timeout: Duration,
        retries: u32,
    ) -> Result<Self> {
        let base_url = Url::parse(base_url).context("Invalid base URL")?;
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            base_url,
            http,
            tenant_auth,
            user_agent: format!("quiltc/{}", env!("CARGO_PKG_VERSION")),
            retries,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn build_url(&self, path: &str) -> Result<Url> {
        self.base_url.join(path).with_context(|| {
            format!(
                "Failed to join base_url={} with path={}",
                self.base_url, path
            )
        })
    }

    pub async fn send_json(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body: Option<serde_json::Value>,
    ) -> Result<()> {
        let bytes = self.send_json_bytes(method, path, headers, body).await?;
        print_bytes(&bytes)?;
        Ok(())
    }

    pub async fn send_json_bytes(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body: Option<serde_json::Value>,
    ) -> Result<Vec<u8>> {
        let url = self.build_url(path)?;

        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            let req_id = Uuid::new_v4().to_string();
            let mut req = self.http.request(method.clone(), url.clone());
            req = req.header("user-agent", &self.user_agent);
            req = req.header("x-request-id", &req_id);

            if let Some(auth) = &self.tenant_auth {
                match auth {
                    TenantAuth::Jwt(jwt) => {
                        req = req.header("authorization", format!("Bearer {}", jwt));
                    }
                    TenantAuth::ApiKey(k) => {
                        req = req.header("x-api-key", k);
                    }
                }
            }

            for (k, v) in headers.iter() {
                req = req.header(k, v);
            }

            if let Some(b) = &body {
                req = req.json(b);
            }

            debug!("HTTP {} {} (attempt {})", method, url, attempt);
            let resp = req.send().await.context("Request failed")?;
            let status = resp.status();

            if status.is_success() {
                let bytes = resp.bytes().await.unwrap_or_default().to_vec();
                return Ok(bytes);
            }

            // Try structured error, fallback to raw.
            let retry_after_header = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let bytes = resp.bytes().await.unwrap_or_default();
            let err = serde_json::from_slice::<ErrorResponse>(&bytes).ok();
            let body_text = if bytes.is_empty() {
                String::new()
            } else {
                String::from_utf8_lossy(&bytes).to_string()
            };

            // Retry handling: 429 with Retry-After, and GET/DELETE on 5xx.
            if attempt <= self.retries && should_retry(&method, status) {
                let sleep_dur = retry_sleep(status, retry_after_header, err.as_ref(), &body_text);
                tokio::time::sleep(sleep_dur).await;
                continue;
            }

            // Final error render
            if let Some(e) = err {
                anyhow::bail!(
                    "HTTP {} {} failed: status={} error_code={:?} error={:?} request_id={:?} retry_after={:?} hint={:?} details={}",
                    method,
                    url,
                    status.as_u16(),
                    e.error_code,
                    e.error,
                    e.request_id,
                    e.retry_after,
                    e.hint,
                    e.details
                        .as_ref()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "null".to_string())
                );
            } else {
                anyhow::bail!(
                    "HTTP {} {} failed: status={} body={}",
                    method,
                    url,
                    status.as_u16(),
                    body_text
                );
            }
        }
    }

    pub async fn stream_to_stdout(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body: Option<serde_json::Value>,
    ) -> Result<()> {
        let url = self.build_url(path)?;

        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            let req_id = Uuid::new_v4().to_string();
            let mut req = self.http.request(method.clone(), url.clone());
            req = req.header("user-agent", &self.user_agent);
            req = req.header("x-request-id", &req_id);

            if let Some(auth) = &self.tenant_auth {
                match auth {
                    TenantAuth::Jwt(jwt) => {
                        req = req.header("authorization", format!("Bearer {}", jwt))
                    }
                    TenantAuth::ApiKey(k) => req = req.header("x-api-key", k),
                }
            }

            for (k, v) in headers.iter() {
                req = req.header(k, v);
            }
            if let Some(b) = &body {
                req = req.json(b);
            }

            debug!("HTTP(stream) {} {} (attempt {})", method, url, attempt);
            let resp = req.send().await.context("Request failed")?;
            let status = resp.status();
            let retry_after_header = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());

            if !status.is_success() {
                let bytes = resp.bytes().await.unwrap_or_default();
                let err = serde_json::from_slice::<ErrorResponse>(&bytes).ok();
                let body_text = String::from_utf8_lossy(&bytes).to_string();

                if attempt <= self.retries && should_retry(&method, status) {
                    let sleep_dur =
                        retry_sleep(status, retry_after_header, err.as_ref(), &body_text);
                    tokio::time::sleep(sleep_dur).await;
                    continue;
                }

                if let Some(e) = err {
                    anyhow::bail!(
                        "HTTP {} {} failed: status={} error_code={:?} error={:?} request_id={:?}",
                        method,
                        url,
                        status.as_u16(),
                        e.error_code,
                        e.error,
                        e.request_id
                    );
                }
                anyhow::bail!(
                    "HTTP {} {} failed: status={} body={}",
                    method,
                    url,
                    status.as_u16(),
                    body_text
                );
            }

            let mut out = std::io::stdout().lock();
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("Stream read failed")?;
                out.write_all(&chunk).context("stdout write failed")?;
                out.flush().ok();
            }
            return Ok(());
        }
    }

    pub async fn download_to_file(
        &self,
        path: &str,
        headers: HeaderMap,
        out_path: &std::path::Path,
    ) -> Result<()> {
        let url = self.build_url(path)?;
        let req_id = Uuid::new_v4().to_string();
        let mut req = self.http.request(Method::GET, url.clone());
        req = req.header("user-agent", &self.user_agent);
        req = req.header("x-request-id", &req_id);

        if let Some(auth) = &self.tenant_auth {
            match auth {
                TenantAuth::Jwt(jwt) => {
                    req = req.header("authorization", format!("Bearer {}", jwt))
                }
                TenantAuth::ApiKey(k) => req = req.header("x-api-key", k),
            }
        }
        for (k, v) in headers.iter() {
            req = req.header(k, v);
        }

        debug!("HTTP(download) GET {}", url);
        let resp = req.send().await.context("Request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.bytes().await.unwrap_or_default();
            anyhow::bail!(
                "Download failed: status={} body={}",
                status.as_u16(),
                String::from_utf8_lossy(&bytes)
            );
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create dir {:?}", parent))?;
        }

        let mut file = std::fs::File::create(out_path)
            .with_context(|| format!("Failed to create {:?}", out_path))?;

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Stream read failed")?;
            file.write_all(&chunk).context("file write failed")?;
        }
        Ok(())
    }
}

fn should_retry(method: &Method, status: StatusCode) -> bool {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return true;
    }
    if status.is_server_error() {
        return matches!(*method, Method::GET | Method::DELETE);
    }
    false
}

fn retry_sleep(
    status: StatusCode,
    retry_after_header: Option<u64>,
    err: Option<&ErrorResponse>,
    body_text: &str,
) -> Duration {
    if status == StatusCode::TOO_MANY_REQUESTS {
        if let Some(s) = retry_after_header {
            return Duration::from_secs(s);
        }
        if let Some(e) = err {
            if let Some(s) = e.retry_after {
                return Duration::from_secs(s);
            }
        }
        // Fallback: try to parse Retry-After from raw body, otherwise 1s.
        let _ = body_text;
        return Duration::from_secs(1);
    }
    Duration::from_millis(300)
}

pub fn header_kv(k: &str, v: &str) -> Result<(HeaderName, HeaderValue)> {
    let name = HeaderName::from_bytes(k.as_bytes()).context("Invalid header name")?;
    let value = HeaderValue::from_str(v).context("Invalid header value")?;
    Ok((name, value))
}

fn print_bytes(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        println!("{}", r#"{"success":true}"#);
        return Ok(());
    }
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) {
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        println!("{}", String::from_utf8_lossy(bytes));
    }
    Ok(())
}
