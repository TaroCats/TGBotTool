/*
 * @Author: taro etsy@live.com
 * @LastEditors: taro etsy@live.com
 * @LastEditTime: 2025-12-09 13:55:17
 * @Description:
 */
use dotenv::dotenv;
use std::env;
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::time::{self, Duration};

mod cloudreve;
use cloudreve::CloudreveClient;
mod bot;
use bot::Command;

use std::collections::HashMap;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();
    log::info!("Starting Cloudreve bot...");

    let token = env::var("BOT_TOKEN").expect("BOT_TOKEN must be set");

    // Read Cloudreve config
    let cr_url = env::var("CLOUDEREVE_API_URL").expect("CLOUDEREVE_API_URL must be set");
    let cr_user = env::var("CLOUDEREVE_USERNAME").expect("CLOUDEREVE_USERNAME must be set");
    let cr_pass = env::var("CLOUDEREVE_PASSWORD").expect("CLOUDEREVE_PASSWORD must be set");

    let client = CloudreveClient::new(cr_url);

    match client.login(&cr_user, &cr_pass).await {
        Ok(_) => log::info!("Cloudreve login successful"),
        Err(e) => {
            log::error!("Cloudreve login failed: {}", e);
        }
    }

    // Spawn token refresh task
    let refresh_client = client.clone();
    tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_secs(25 * 60)).await;
            log::info!("Refreshing Cloudreve token...");
            match refresh_client.refresh_token().await {
                Ok(_) => log::info!("Token refreshed successfully"),
                Err(e) => log::error!("Failed to refresh token: {}", e),
            }
        }
    });

    let bot_instance = Bot::new(token);
    let client = Arc::new(client);
    // Initialize page token cache
    let cache: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));

    let handler = Update::filter_message()
        .branch(
            dptree::filter(|msg: Message| msg.text().map(|t| t.starts_with('/')).unwrap_or(false))
                .filter_command::<Command>()
                .endpoint(
                    |bot: Bot,
                     msg: Message,
                     cmd: Command,
                     client: Arc<CloudreveClient>,
                     cache: Arc<Mutex<HashMap<String, String>>>| async move {
                        bot::answer(bot, msg, cmd, client, cache).await
                    },
                ),
        )
        .branch(dptree::endpoint(|bot: Bot, msg: Message| async move {
            bot::message_handler(bot, msg).await
        }));

    let callback_handler = Update::filter_callback_query().endpoint(
        |bot: Bot,
         q: CallbackQuery,
         client: Arc<CloudreveClient>,
         cache: Arc<Mutex<HashMap<String, String>>>| async move {
            bot::callback_handler(bot, q, client, cache).await
        },
    );

    Dispatcher::builder(
        bot_instance,
        dptree::entry().branch(handler).branch(callback_handler),
    )
    .dependencies(dptree::deps![client, cache])
    .enable_ctrlc_handler()
    .build()
    .dispatch()
    .await;
}
