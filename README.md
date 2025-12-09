# Cloudreve Telegram Bot

A Telegram bot to interact with Cloudreve.

## Features
- Login to Cloudreve
- List files (`/ls [path]`)

## Configuration
The bot uses `.env` file for configuration:
```env
BOT_TOKEN=your_telegram_bot_token
CLOUDEREVE_API_URL=https://your-cloudreve-instance.com
CLOUDEREVE_USERNAME=your_username
CLOUDEREVE_PASSWORD=your_password
```

## Running
```bash
cargo run
```
