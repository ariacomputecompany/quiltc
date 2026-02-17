use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorResponse {
    pub error: Option<String>,
    pub error_code: Option<String>,
    pub request_id: Option<String>,
    pub details: Option<Value>,
    pub retry_after: Option<u64>,
    pub hint: Option<String>,
}
