use crate::cloudreve::{CloudreveClient, DirectoryResponse};
use anyhow::{anyhow, Result};

impl CloudreveClient {
    pub async fn list_file_source(&self, uri: &str) -> Result<String> {
        let body = serde_json::json!({
            "uris": [uri],
        });

        let resp = self
            .request_builder(reqwest::Method::PUT, "/file/source")
            .await
            .json(&body)
            .send()
            .await?;

        let text = resp.text().await?;

        let api_resp: DirectoryResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse directory response: {} - {}", e, text))?;

        if api_resp.code == 0 {
            if let Some(data) = api_resp.data {
                if let Some(files) = data.as_array() {
                    if let Some(first_file) = files.first() {
                        if let Some(url) = first_file.get("url").and_then(|v| v.as_str()) {
                            return Ok(url.to_string());
                        }
                    }
                }
            }
            Err(anyhow!("No file source found"))
        } else {
            Err(anyhow!(
                "Failed to list files: {}",
                api_resp.msg.unwrap_or_default()
            ))
        }
    }
}
