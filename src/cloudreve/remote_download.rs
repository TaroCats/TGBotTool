use crate::cloudreve::{CloudreveClient, DirectoryResponse};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::env;

impl CloudreveClient {
    pub async fn remote_download(&self, url: &str) -> Result<Value> {
        let dst = env::var("CLOUDEREVE_DOWNLOAD_PATH")?;
        let body = serde_json::json!({
            "dst": dst,
            "src": [&url]
        });
        let resp = self
            .request_builder(reqwest::Method::POST, "/workflow/download")
            .await
            .json(&body)
            .send()
            .await?;

        let text = resp.text().await?;

        let api_resp: DirectoryResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse directory response: {} - {}", e, text))?;

        if api_resp.code == 0 {
            Ok(api_resp.data.unwrap_or_default())
        } else {
            Err(anyhow!(
                "Failed to download file: {}",
                api_resp.msg.unwrap_or_default()
            ))
        }
    }
}
