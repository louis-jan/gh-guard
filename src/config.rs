use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "gh-guard";
const PAT_USER: &str = "github-pat";
const TG_TOKEN_USER: &str = "telegram-bot-token";
const TG_CHAT_USER: &str = "telegram-chat-id";

// ── GitHub PAT ───────────────────────────────────────────────────────────────

pub fn get_pat() -> Result<String> {
    Entry::new(SERVICE, PAT_USER)
        .context("Cannot open macOS Keychain")?
        .get_password()
        .context("GitHub PAT not found. Run `gh-guard setup` first.")
}

pub fn set_pat(token: &str) -> Result<()> {
    Entry::new(SERVICE, PAT_USER)
        .context("Cannot open macOS Keychain")?
        .set_password(token)
        .context("Failed to store PAT in macOS Keychain")
}

// ── Telegram ─────────────────────────────────────────────────────────────────

pub fn get_telegram_token() -> Result<String> {
    Entry::new(SERVICE, TG_TOKEN_USER)
        .context("Cannot open macOS Keychain")?
        .get_password()
        .context("Telegram bot token not found. Run `gh-guard setup` first.")
}

pub fn set_telegram_token(token: &str) -> Result<()> {
    Entry::new(SERVICE, TG_TOKEN_USER)
        .context("Cannot open macOS Keychain")?
        .set_password(token)
        .context("Failed to store Telegram token in macOS Keychain")
}

pub fn get_telegram_chat_id() -> Result<String> {
    Entry::new(SERVICE, TG_CHAT_USER)
        .context("Cannot open macOS Keychain")?
        .get_password()
        .context("Telegram chat ID not found. Run `gh-guard setup` first.")
}

pub fn set_telegram_chat_id(id: &str) -> Result<()> {
    Entry::new(SERVICE, TG_CHAT_USER)
        .context("Cannot open macOS Keychain")?
        .set_password(id)
        .context("Failed to store Telegram chat ID in macOS Keychain")
}
