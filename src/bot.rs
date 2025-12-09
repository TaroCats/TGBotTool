/*
 * @Author: taro etsy@live.com
 * @LastEditors: taro etsy@live.com
 * @LastEditTime: 2025-12-09 15:20:34
 * @Description:
 */
use crate::cloudreve::CloudreveClient;
use anyhow::{anyhow, Result};
use log::info;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;
use teloxide::types::{
    InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage, MessageId,
    ReplyParameters,
};
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;
use tokio::time::sleep;

// Simple in-memory cache for page tokens: Map<"path:page", next_token>
// Note: This is a simple implementation. In production, use a TTL cache (e.g., moka) to avoid memory leaks.
type PageTokenCache = Arc<Mutex<HashMap<String, String>>>;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "List files in the current or specified directory.")]
    List(String),
}

// Handler for commands
pub async fn answer(
    bot: Bot,
    msg: Message,
    cmd: Command,
    client: Arc<CloudreveClient>,
    cache: PageTokenCache,
) -> ResponseResult<()> {
    match cmd {
        Command::List(path) => {
            let path = path.trim();
            let path = if path.is_empty() {
                env::var("CLOUDEREVE_BASE_PATH").unwrap_or_else(|_| "cloudreve://my".to_string())
            } else {
                path.to_string()
            };

            let page = 0;
            list_files_and_send(&bot, msg.chat.id, &client, &path, page, cache, None).await?;
        }
    };
    Ok(())
}

pub async fn get_download_status(
    bot: Bot,
    chat_id: ChatId,
    message_id: MessageId,
    url: &str,
    client: Arc<CloudreveClient>,
) -> Result<()> {
    let url = url.to_string();

    // Send initial message
    let status_msg = bot
        .edit_message_text(chat_id, message_id, "正在获取下载状态...")
        .await?;

    let chat_id = status_msg.chat.id;
    let message_id = status_msg.id;

    loop {
        sleep(Duration::from_secs(5)).await;
        bot.edit_message_text(chat_id, message_id, "正在获取下载状态...")
            .await?;
        match client.search_remote_list_by_url("downloading", &url).await {
            Ok(resp) => {
                let name = resp
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                let size_str = resp
                    .get("size_str")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                let progress_str = resp
                    .get("progress_str")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0");

                let text = format!(
                    "<b>{}</b>\n文件大小: {}\n下载进度: {}%",
                    name, size_str, progress_str
                );

                // Edit the message
                match bot
                    .edit_message_text(chat_id, message_id, text)
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        // Ignore "message is not modified" errors, log others
                        if !e.to_string().contains("message is not modified") {
                            info!("Failed to update status message: {}", e);
                        }
                    }
                }

                if progress_str == "100" {
                    break;
                }
            }
            Err(e) => {
                info!("搜索下载失败: {}", e);
            }
        }
    }
    Ok(())
}

pub async fn send_remote_download(
    bot: Bot,
    msg: Message,
    url: &str,
    client: Arc<CloudreveClient>,
) -> Result<()> {
    match client.remote_download(url).await {
        Ok(_) => {
            let sent_msg = bot
                .edit_message_text(msg.chat.id, msg.id, "远程下载成功".to_string())
                .await?;
            let _ = get_download_status(
                bot.clone(),
                sent_msg.chat.id,
                sent_msg.id,
                url,
                client.clone(),
            )
            .await;
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("远程下载失败: {}", e))
                .reply_parameters(ReplyParameters::new(msg.id))
                .await?;
        }
    }
    Ok(())
}

// Handler for callback queries (button clicks)
pub async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    client: Arc<CloudreveClient>,
    cache: PageTokenCache,
) -> ResponseResult<()> {
    if let Some(data) = q.data {
        let parts: Vec<&str> = data.splitn(2, ':').collect();
        if parts.len() < 2 {
            return Ok(());
        }
        let action = parts[0];
        let payload = parts[1];

        bot.answer_callback_query(q.id)
            .send()
            .await
            .log_on_error()
            .await;

        match action {
            "cd" => {
                let path = payload;
                if let Some(msg) = q.message {
                    list_files_and_send(
                        &bot,
                        msg.chat().id,
                        &client,
                        path,
                        0,
                        cache,
                        Some(msg.id()),
                    )
                    .await?;
                }
            }
            "pg" => {
                if let Some(last_colon) = payload.rfind(':') {
                    let path = &payload[0..last_colon];
                    if let Ok(page) = payload[last_colon + 1..].parse::<u32>() {
                        if let Some(msg) = q.message {
                            // Pass message ID for editing
                            list_files_and_send(
                                &bot,
                                msg.chat().id,
                                &client,
                                path,
                                page,
                                cache,
                                Some(msg.id()),
                            )
                            .await?;
                        }
                    }
                }
            }
            "gl" => {
                let path = payload;
                if let Some(msg) = q.message {
                    let source = client.list_file_source(path).await;
                    let url = match source {
                        Ok(url) => url,
                        Err(e) => {
                            let text = format!("获取下载链接失败: {}", e);
                            if let Some(mid) = Some(msg.id()) {
                                bot.edit_message_text(msg.chat().id, mid, text).await?;
                            } else {
                                bot.send_message(msg.chat().id, text).await?;
                            }
                            return Ok(());
                        }
                    };
                    let text = format!("单击下面的链接可直接复制：\n<code>{}</code>", url);

                    if let Some(mid) = Some(msg.id()) {
                        bot.edit_message_text(msg.chat().id, mid, text)
                            .parse_mode(teloxide::types::ParseMode::Html)
                            .await?;
                    } else {
                        bot.send_message(msg.chat().id, text)
                            .parse_mode(teloxide::types::ParseMode::Html)
                            .await?;
                    }
                }
            }
            "rd" => {
                let path = payload;
                if let Some(MaybeInaccessibleMessage::Regular(message)) = q.message {
                    let _ =
                        send_remote_download(bot.clone(), message.clone(), path, client.clone())
                            .await;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub async fn list_files_and_send(
    bot: &Bot,
    chat_id: ChatId,
    client: &CloudreveClient,
    path: &str,
    page: u32,
    cache: PageTokenCache,
    message_id: Option<MessageId>,
) -> ResponseResult<()> {
    let page_size = 10;

    let token_to_use = if page > 0 {
        let key = format!("{}:{}", path, page);
        let cache_lock = cache.lock().await;
        cache_lock.get(&key).cloned().unwrap_or_default()
    } else {
        String::new()
    };

    match client
        .list_files()
        .uri(path)
        .page(page)
        .page_size(page_size)
        .next_page_token(&token_to_use)
        .send()
        .await
    {
        Ok((data, next_token)) => {
            // Handle data as Value
            let files_opt = if let Some(files) = data.get("files").and_then(|f| f.as_array()) {
                Some(files)
            } else {
                data.as_array()
            };
            let pagination = data.get("pagination").and_then(|v| v.as_object());
            if let Some(next_token) = pagination
                .and_then(|v| v.get("next_token"))
                .and_then(|v| v.as_str())
            {
                if next_token.is_empty() {
                } else {
                    let key = format!("{}:{}", path, page + 1);
                    let mut cache_lock = cache.lock().await;
                    cache_lock.insert(key, next_token.to_string());
                }
            }
            if let Some(files) = files_opt {
                if files.is_empty() && page == 0 {
                    let text = format!("Directory `{}` is empty.", path);
                    if let Some(mid) = message_id {
                        bot.edit_message_text(chat_id, mid, text).await?;
                    } else {
                        bot.send_message(chat_id, text).await?;
                    }
                } else {
                    let mut buttons = Vec::new();

                    for file in files {
                        let file_type = file
                            .get("type")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_default();
                        let name = file
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown");

                        let icon = if file_type == 1 { "📁" } else { "📄" };
                        let display_text = format!("{} {}", icon, name);
                        let full_path = file.get("path").and_then(|v| v.as_str()).unwrap_or(name);
                        let callback_data = if file_type == 1 {
                            format!("cd:{}", full_path)
                        } else {
                            format!("gl:{}", full_path)
                        };

                        if callback_data.len() <= 64 {
                            buttons.push(vec![InlineKeyboardButton::callback(
                                display_text,
                                callback_data,
                            )]);
                        } else {
                            buttons
                                .push(vec![InlineKeyboardButton::callback(display_text, "noop")]);
                        }
                    }

                    // Pagination buttons
                    let mut pagination_row = Vec::new();
                    if page > 0 {
                        pagination_row.push(InlineKeyboardButton::callback(
                            "⬅️ Prev",
                            format!("pg:{}:{}", path, page - 1),
                        ));
                    }

                    if next_token.is_some() || files.len() as u32 == page_size {
                        pagination_row.push(InlineKeyboardButton::callback(
                            "Next ➡️",
                            format!("pg:{}:{}", path, page + 1),
                        ));
                    }

                    if !pagination_row.is_empty() {
                        buttons.push(pagination_row);
                    }

                    let keyboard = InlineKeyboardMarkup::new(buttons);
                    let text = format!("Files in `{}` (Page {}):", path, page);

                    if let Some(mid) = message_id {
                        bot.edit_message_text(chat_id, mid, text)
                            .reply_markup(keyboard)
                            .await?;
                    } else {
                        bot.send_message(chat_id, text)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            } else {
                let text = "Failed to parse file list.";
                if let Some(mid) = message_id {
                    bot.edit_message_text(chat_id, mid, text).await?;
                } else {
                    bot.send_message(chat_id, text).await?;
                }
            }
        }
        Err(e) => {
            let text = format!("Error listing files: {}", e);
            if let Some(mid) = message_id {
                bot.edit_message_text(chat_id, mid, text).await?;
            } else {
                bot.send_message(chat_id, text).await?;
            }
        }
    }
    Ok(())
}

pub async fn answer_message_by_link(bot: Bot, msg: Message, url: &str) -> ResponseResult<()> {
    let source_link = match get_source_link(url).await {
        Ok(source_link) => source_link,
        Err(e) => format!("解析链接失败: {}", e),
    };
    let text = format!("单击下面的链接可直接复制：\n<code>{}</code>", source_link);
    let mut buttons = Vec::new();
    buttons.push(vec![InlineKeyboardButton::callback(
        "提交云盘下载",
        format!("rd:{}", source_link),
    )]);
    let keyboard = InlineKeyboardMarkup::new(buttons);
    bot.send_message(msg.chat.id, text)
        .parse_mode(teloxide::types::ParseMode::Html)
        .reply_parameters(ReplyParameters::new(msg.id))
        .reply_markup(keyboard)
        .await?;
    Ok(())
}

pub async fn message_handler(bot: Bot, msg: Message) -> ResponseResult<()> {
    if let Some(text) = msg.text() {
        for entity in msg.entities().unwrap_or(&[]) {
            if let teloxide::types::MessageEntityKind::Url
            | teloxide::types::MessageEntityKind::TextLink { .. } = &entity.kind
            {
                let url = match &entity.kind {
                    teloxide::types::MessageEntityKind::Url => {
                        text[entity.offset..(entity.offset + entity.length)].to_string()
                    }
                    teloxide::types::MessageEntityKind::TextLink { url } => url.to_string(),
                    _ => continue,
                };
                if url.contains("t.me") {
                    answer_message_by_link(bot.clone(), msg.clone(), &url).await?;
                }
            }
        }
    }
    if let Some(origin) = msg.forward_origin() {
        match origin {
            teloxide::types::MessageOrigin::Channel {
                chat, message_id, ..
            } => {
                if let Some(username) = chat.username() {
                    let url = format!("https://t.me/{}/{}", username, message_id);
                    answer_message_by_link(bot.clone(), msg.clone(), &url).await?;
                }
            }
            _ => {
                bot.send_message(msg.chat.id, "无法解析来源链接".to_string())
                    .reply_parameters(ReplyParameters::new(msg.id))
                    .await?;
            }
        }
    }
    Ok(())
}

pub async fn get_source_link(url: &str) -> Result<String> {
    let resp = reqwest::get(format!("{}/api/resolve?url={}", "https://tg.taro.cat", url)).await?;
    let text = resp.text().await?;
    let api_resp: Value = serde_json::from_str(&text)
        .map_err(|e| anyhow!("Failed to parse response: {} - {}", e, text))?;
    if !api_resp
        .get("ok")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(anyhow!("Failed to resolve URL"));
    }
    if let Some(download_url) = api_resp.get("stream_link").and_then(|v| v.as_str()) {
        Ok(download_url.to_string())
    } else {
        Err(anyhow!("Failed to resolve URL"))
    }
}
