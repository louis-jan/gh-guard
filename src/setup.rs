use anyhow::{anyhow, bail, Result};
use colored::Colorize;
use std::io::{self, Write};
use std::time::{Duration, Instant};

pub fn run(sub: Option<&str>) -> Result<()> {
    match sub {
        Some("test") => test_notification(),
        Some("show") => show_config(),
        Some("pat") => wizard_pat_only(),
        Some("telegram") => wizard_telegram_only(),
        Some(unknown) => bail!("Unknown setup subcommand: {unknown}"),
        None => wizard_full(),
    }
}

// ── Full wizard ───────────────────────────────────────────────────────────────

fn wizard_full() -> Result<()> {
    println!("{}", "╔══════════════════════════════════╗".cyan());
    println!("{}", "║      gh-guard  ·  Setup          ║".cyan().bold());
    println!("{}", "╚══════════════════════════════════╝".cyan());
    println!();

    wizard_pat_only()?;
    println!();
    wizard_telegram_only()?;
    println!();

    println!("{}", "Setup complete!".green().bold());
    println!();
    println!("Add this alias to your shell config (~/.zshrc or ~/.bashrc):");
    println!("  {}", "alias gh='gh-guard'".cyan().bold());
    println!();
    println!("Run {} to send a test message to your phone.", "gh-guard setup test".cyan());
    Ok(())
}

// ── GitHub PAT sub-wizard ─────────────────────────────────────────────────────

fn wizard_pat_only() -> Result<()> {
    println!("{}", "── GitHub Personal Access Token ──".bold());
    println!("Create one at:  https://github.com/settings/tokens");
    println!("Required scopes: {}", "repo, read:org".yellow());
    println!(
        "(Tip: run {} to copy your current gh session token.)",
        "gh auth token | pbcopy".cyan()
    );
    println!();

    let pat = rpassword::prompt_password("PAT (input hidden): ")?;
    let pat = pat.trim().to_string();
    if pat.is_empty() {
        bail!("PAT cannot be empty.");
    }

    print!("Validating… ");
    io::stdout().flush()?;
    match validate_pat(&pat) {
        Ok(login) => println!("{} (signed in as {})", "✓".green(), login.bold()),
        Err(e) => {
            println!("{}", "✗".red());
            bail!("{e}\nCheck your token and try again.");
        }
    }

    crate::config::set_pat(&pat)?;
    println!("{}", "PAT stored in macOS Keychain.".green());
    Ok(())
}

// ── Telegram sub-wizard ───────────────────────────────────────────────────────

fn wizard_telegram_only() -> Result<()> {
    println!("{}", "── Telegram Bot ──".bold());
    println!("1. Open Telegram and message {}", "@BotFather".cyan());
    println!("2. Send {} and follow the prompts to create a bot", "/newbot".cyan());
    println!("3. Copy the token it gives you (format: {})", "123456789:ABCdef…".yellow());
    println!();

    let token = rpassword::prompt_password("Bot token (input hidden): ")?;
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("Token cannot be empty.");
    }

    print!("Validating… ");
    io::stdout().flush()?;
    let bot_username = match get_bot_info(&token) {
        Ok(name) => {
            println!("{} (bot is @{})", "✓".green(), name.bold());
            name
        }
        Err(e) => {
            println!("{}", "✗".red());
            bail!("{e}\nCheck the token and try again.");
        }
    };

    crate::config::set_telegram_token(&token)?;
    println!("{}", "Bot token stored in macOS Keychain.".green());
    println!();

    // Auto-detect chat ID by waiting for the user to send a message to the bot.
    println!(
        "Now send any message to {} in Telegram.",
        format!("@{bot_username}").cyan().bold()
    );
    println!("Waiting up to 2 minutes…");

    let chat_id = detect_chat_id(&token)?;

    crate::config::set_telegram_chat_id(&chat_id)?;
    println!("{}", format!("Chat ID {chat_id} stored in macOS Keychain.").green());
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(40))
        .build()
}

fn tg(token: &str, method: &str) -> String {
    format!("https://api.telegram.org/bot{token}/{method}")
}

fn validate_pat(pat: &str) -> Result<String> {
    let resp: serde_json::Value = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build()
        .get("https://api.github.com/user")
        .set("Authorization", &format!("Bearer {pat}"))
        .set("User-Agent", "gh-guard/0.1")
        .call()
        .map_err(|e| anyhow!("GitHub API: {e}"))?
        .into_json()?;

    Ok(resp["login"].as_str().unwrap_or("unknown").to_string())
}

fn get_bot_info(token: &str) -> Result<String> {
    let resp: serde_json::Value = make_agent()
        .get(&tg(token, "getMe"))
        .call()
        .map_err(|e| anyhow!("Telegram API: {e}"))?
        .into_json()?;

    if !resp["ok"].as_bool().unwrap_or(false) {
        return Err(anyhow!(
            "Telegram error: {}",
            resp["description"].as_str().unwrap_or("invalid token")
        ));
    }

    Ok(resp["result"]["username"]
        .as_str()
        .unwrap_or("unknown")
        .to_string())
}

/// Poll getUpdates waiting for the user to send any message to the bot.
/// Returns the chat ID as a string once a message arrives.
fn detect_chat_id(token: &str) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    let a = make_agent();
    let mut offset: Option<i64> = None;

    loop {
        let remaining_secs = if Instant::now() < deadline {
            (deadline - Instant::now()).as_secs()
        } else {
            0
        };
        if remaining_secs == 0 {
            break;
        }

        let poll_timeout = remaining_secs.min(30);
        let mut req = serde_json::json!({
            "timeout": poll_timeout,
            "allowed_updates": ["message"]
        });
        if let Some(off) = offset {
            req["offset"] = serde_json::json!(off);
        }

        let resp: serde_json::Value = a
            .post(&tg(token, "getUpdates"))
            .set("Content-Type", "application/json")
            .send_json(&req)
            .map_err(|e| anyhow!("Network error: {e}"))?
            .into_json()?;

        if let Some(updates) = resp["result"].as_array() {
            for update in updates {
                let update_id = update["update_id"].as_i64().unwrap_or(0);
                let next = update_id + 1;
                offset = Some(offset.map_or(next, |prev| prev.max(next)));

                if let Some(msg) = update.get("message") {
                    if let Some(chat_id) = msg["chat"]["id"].as_i64() {
                        let from = msg["from"]["first_name"].as_str().unwrap_or("?");
                        println!(
                            "{} Got message from {} — chat ID: {}",
                            "✓".green(),
                            from.bold(),
                            chat_id.to_string().bold()
                        );
                        return Ok(chat_id.to_string());
                    }
                }
            }
        }
    }

    Err(anyhow!(
        "Timed out (2 min) waiting for your message.\n\
         Run `gh-guard setup telegram` to try again."
    ))
}

fn test_notification() -> Result<()> {
    let token = crate::config::get_telegram_token()?;
    let chat_id = crate::config::get_telegram_chat_id()?;

    println!("Sending test message to Telegram…");

    let payload = serde_json::json!({
        "chat_id": chat_id,
        "text": "👋 <b>gh-guard</b> · test notification\n\nSetup is working correctly!",
        "parse_mode": "HTML"
    });

    let resp: serde_json::Value = make_agent()
        .post(&tg(&token, "sendMessage"))
        .set("Content-Type", "application/json")
        .send_json(&payload)?
        .into_json()?;

    if resp["ok"].as_bool().unwrap_or(false) {
        println!("{}", "Sent! Check your Telegram.".green());
    } else {
        bail!(
            "Telegram error: {}",
            resp["description"].as_str().unwrap_or("?")
        );
    }
    Ok(())
}

fn show_config() -> Result<()> {
    println!("{}", "gh-guard configuration".bold());
    println!("{}", "──────────────────────".dimmed());

    match crate::config::get_pat() {
        Ok(pat) => {
            let start = pat.len().min(7);
            let end = pat.len().saturating_sub(4);
            let masked = if end > start {
                format!("{}…{}", &pat[..start], &pat[end..])
            } else {
                format!("{}…", &pat[..start])
            };
            println!("  GitHub PAT      {}", masked.green());
        }
        Err(_) => println!("  GitHub PAT      {}", "not configured".red()),
    }

    match crate::config::get_telegram_token() {
        Ok(token) => {
            let masked = format!("{}…", &token[..token.len().min(10)]);
            println!("  Telegram token  {}", masked.green());
        }
        Err(_) => println!("  Telegram token  {}", "not configured".red()),
    }

    match crate::config::get_telegram_chat_id() {
        Ok(id) => println!("  Telegram chat   {}", id.green()),
        Err(_) => println!("  Telegram chat   {}", "not configured".red()),
    }

    Ok(())
}
