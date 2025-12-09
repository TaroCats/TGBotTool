use std::{thread::sleep, time::Duration};

/*
 * @Author: taro etsy@live.com
 * @LastEditors: taro etsy@live.com
 * @LastEditTime: 2025-12-09 14:15:46
 * @Description:
 */
use crate::cloudreve::{CloudreveClient, DirectoryResponse};
use anyhow::{anyhow, Result};
use log::info;
use serde_json::Value;

// 搜索JSON对象中的指定键值对
#[allow(dead_code)]
pub fn object_array_search(array: &[Value], key: &str, target_value: &str) -> Option<Value> {
    for item in array {
        if let Some(item_obj) = item.as_object() {
            if let Some(value) = item_obj.get(key) {
                // Check if value is a string and matches target_value
                if let Some(s) = value.as_str() {
                    if s == target_value {
                        return Some(item.clone());
                    }
                }
            }
        }
    }
    None
}
impl CloudreveClient {
    pub async fn remote_list(&self, category: &str) -> Result<Value> {
        let resp = self
            .request_builder(reqwest::Method::GET, "/workflow")
            .await
            .query(&[("category", category), ("page_size", "100")])
            .send()
            .await?;

        let text = resp.text().await?;

        let api_resp: DirectoryResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse directory response: {} - {}", e, text))?;

        if api_resp.code == 0 {
            Ok(api_resp.data.unwrap_or_default())
        } else {
            Err(anyhow!(
                "Failed to list directory: {}",
                api_resp.msg.unwrap_or_default()
            ))
        }
    }

    pub async fn search_remote_list_by_url(&self, category: &str, url: &str) -> Result<Value> {
        // 使用 Box::pin 包装递归调用以避免无限大的 Future
        Box::pin(async move {
            if url.is_empty() || category.is_empty() {
                return Err(anyhow!("URL or category is empty"));
            }
            let data = self.remote_list(category).await?;

            if let Some(tasks) = data.get("tasks").and_then(|t| t.as_array()) {
                for task in tasks {
                    if let Some(summary) = task.get("summary").and_then(|s| s.as_object()) {
                        if let Some(props) = summary.get("props").and_then(|p| p.as_object()) {
                            if let Some(src_str) = props.get("src_str").and_then(|s| s.as_str()) {
                                if src_str == url {
                                    info!("Found task: {:?}", url);
                                    if let Some(download) =
                                        props.get("download").and_then(|d| d.as_object())
                                    {
                                        let name = download
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        let size = download
                                            .get("size")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or_default();
                                        let empty_files = Vec::new();
                                        let files = download
                                            .get("files")
                                            .and_then(|v| v.as_array())
                                            .unwrap_or(&empty_files);
                                        let size_str =
                                            format!("{:.2} MB", size as f64 / 1024.0 / 1024.0);
                                        let progress = object_array_search(files, "name", name)
                                            .and_then(|v| v.get("progress").cloned());

                                        let progress_val =
                                            progress.and_then(|v| v.as_f64()).unwrap_or(0.0);
                                        let val = progress_val * 100.0;
                                        let progress_str = if (val - 100.0).abs() < f64::EPSILON {
                                            "100".to_string()
                                        } else {
                                            format!("{:.2}", val)
                                        };

                                        let mut download_obj = download.clone();
                                        download_obj.insert(
                                            "name".to_string(),
                                            serde_json::Value::String(name.to_string()),
                                        );
                                        download_obj.insert(
                                            "size".to_string(),
                                            serde_json::Value::String(size_str),
                                        );
                                        download_obj.insert(
                                            "progress".to_string(),
                                            serde_json::Value::String(progress_str),
                                        );

                                        return Ok(serde_json::Value::Object(download_obj));
                                    } else {
                                        sleep(Duration::from_secs(5));
                                        // 递归调用时再次使用 Box::pin
                                        let download_obj =
                                            Box::pin(self.search_remote_list_by_url(category, url))
                                                .await?;
                                        return Ok(download_obj);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Err(anyhow!("Task not found"))
        })
        .await
    }
}
