use anyhow::{anyhow, Result};
use log::{error, info};
mod request;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

mod list_files;
mod file_source;
mod remote_list;
mod remote_download;

#[derive(Default, Debug)]
struct ClientState {
    token: String,
    refresh_token: String,
}

#[derive(Clone)]
pub struct CloudreveClient {
    client: Client,
    base_url: String,
    state: Arc<RwLock<ClientState>>,
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    #[serde(rename = "email")]
    email: &'a str,
    #[serde(rename = "Password")]
    password: &'a str,
}

#[derive(Deserialize, Debug)]
struct ApiResponse {
    code: i32,
    msg: Option<String>,
    data: Option<Value>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct DirectoryResponse {
    code: i32,
    data: Option<Value>,
    msg: Option<String>,
}

impl CloudreveClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder().cookie_store(true).build().unwrap();

        Self {
            client,
            base_url: format!("{}/api/v4", base_url.trim_end_matches('/')),
            state: Arc::new(RwLock::new(ClientState::default())),
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<()> {
        let url = format!("{}/session/token", self.base_url);
        let body = LoginRequest {
            email: username,
            password: password,
        };

        let resp = self.client.post(&url).json(&body).send().await?;

        let text = resp.text().await?;

        let api_resp: ApiResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse login response: {} - {}", e, text))?;

        if api_resp.code == 0 {
            info!("Login successful");
            if let Some(data) = api_resp.data {
                // Extract tokens based on V4 structure: data.token.access_token
                let mut state = self.state.write().await;

                // Inspect if "data" itself has token fields or if it's nested
                if let Some(token_obj) = data.get("token") {
                    if let Some(token) = token_obj.get("access_token").and_then(|v| v.as_str()) {
                        state.token = token.to_string();
                        info!("Got access token");
                    }
                    if let Some(rt) = token_obj.get("refresh_token").and_then(|v| v.as_str()) {
                        state.refresh_token = rt.to_string();
                        info!("Got refresh token");
                    }
                } else {
                    // Fallback to flat structure if API differs
                    if let Some(token) = data.get("token").and_then(|v| v.as_str()) {
                        state.token = token.to_string();
                    }
                }
            }
            Ok(())
        } else {
            error!("Login failed: {:?}", api_resp);
            Err(anyhow!(
                "Login failed: {}",
                api_resp.msg.unwrap_or_default()
            ))
        }
    }

    pub async fn refresh_token(&self) -> Result<()> {
        let (refresh_token, _token) = {
            let state = self.state.read().await;
            (state.refresh_token.clone(), state.token.clone())
        };

        if refresh_token.is_empty() {
            return Err(anyhow!("No refresh token available"));
        }

        let resp = self
            .request_builder(reqwest::Method::POST, "/session/token/refresh")
            .await
            .send()
            .await?;

        let text = resp.text().await?;
        let api_resp: ApiResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse refresh response: {} - {}", e, text))?;

        if api_resp.code == 0 {
            info!("Token refresh successful");
            if let Some(data) = api_resp.data {
                let mut state = self.state.write().await;
                // Check nested structure again
                if let Some(token_obj) = data.get("token") {
                    if let Some(token) = token_obj.get("access_token").and_then(|v| v.as_str()) {
                        state.token = token.to_string();
                    }
                    if let Some(rt) = token_obj.get("refresh_token").and_then(|v| v.as_str()) {
                        state.refresh_token = rt.to_string();
                    }
                }
            }
            Ok(())
        } else {
            Err(anyhow!(
                "Refresh failed: {}",
                api_resp.msg.unwrap_or_default()
            ))
        }
    }
}
