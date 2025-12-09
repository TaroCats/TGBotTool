/*
 * @Author: taro etsy@live.com
 * @LastEditors: taro etsy@live.com
 * @LastEditTime: 2025-12-09 08:49:12
 * @Description:
 */
use crate::cloudreve::{CloudreveClient, DirectoryResponse};
use anyhow::{anyhow, Result};
use log::info;
use serde_json::Value;
use std::env;

pub struct ListFilesBuilder<'a> {
    pub(crate) client: &'a CloudreveClient,
    pub(crate) uri: String,
    pub(crate) page: Option<u32>,
    pub(crate) page_size: Option<u32>,
    pub(crate) next_page_token: Option<String>,
}

impl<'a> ListFilesBuilder<'a> {
    pub fn new(client: &'a CloudreveClient) -> Self {
        Self {
            client,
            uri: String::new(),
            page: None,
            page_size: None,
            next_page_token: None,
        }
    }

    pub fn uri(mut self, uri: &str) -> Self {
        self.uri = uri.to_string();
        self
    }

    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    pub fn page_size(mut self, page_size: u32) -> Self {
        self.page_size = Some(page_size);
        self
    }

    pub fn next_page_token(mut self, token: &str) -> Self {
        self.next_page_token = Some(token.to_string());
        self
    }

    pub async fn send(self) -> Result<(Value, Option<String>)> {
        let uri = if self.uri.is_empty() {
            env::var("CLOUDEREVE_BASE_PATH").unwrap_or_else(|_| "cloudreve://my".to_string())
        } else {
            self.uri
        };
        info!("Listing files in uri: {}", uri);
        let path_url = "/file";

        let resp = self
            .client
            .request_builder(reqwest::Method::GET, path_url)
            .await
            .query(&[("uri", &uri)])
            .query(&[(
                "page",
                &self.page.map_or("0".to_string(), |p| p.to_string()),
            )])
            .query(&[(
                "page_size",
                &self.page_size.map_or("50".to_string(), |ps| ps.to_string()),
            )])
            .query(&[("next_page_token", &self.next_page_token.unwrap_or_default())])
            .send()
            .await?;

        let text = resp.text().await?;

        let api_resp: DirectoryResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse directory response: {} - {}", e, text))?;

        if api_resp.code == 0 {
            let data = api_resp.data.unwrap_or(Value::Null);
            // Try to extract next_page_token from pagination object in data
            let next_token = data
                .get("pagination")
                .and_then(|p| p.get("next_page_token")) // Standard might be page_token? Check doc.
                // Re-checking doc: "next_page_token" is query param, response usually has it in meta or similar.
                // Assuming standard cursor pagination where it might be in pagination object.
                // Based on previous search, it seemed to be a query param for request.
                // Let's assume it's in data.pagination.next_page_token or similar.
                // Or maybe just check if we have more pages?
                // For now, let's extract it if present.
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Wait, standard Cloudreve list response usually has objects.
            // If it's cursor based, it might be in the response props.
            // Let's return the whole data value and let the caller parse the token for now,
            // OR extracting it here is better.

            // Let's try to find it in "pagination" object inside data.
            // data: { files: [...], pagination: { page: ..., page_size: ..., next_page_token: ...? } }
            // If it's cursor based.

            // Actually, based on my previous read of V4 docs (from memory/search):
            // It might be data.pagination.next_page_token

            Ok((data, next_token))
        } else {
            Err(anyhow!(
                "Failed to list files: {}",
                api_resp.msg.unwrap_or_default()
            ))
        }
    }
}

impl CloudreveClient {
    pub fn list_files(&self) -> ListFilesBuilder<'_> {
        ListFilesBuilder::new(self)
    }
}
