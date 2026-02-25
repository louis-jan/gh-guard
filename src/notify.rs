use anyhow::{anyhow, Context, Result};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub enum ApprovalResult {
    Approved,
    Rejected,
    Timeout,
}

pub struct TgConfig {
    pub token: String,
    pub chat_id: String,
}

impl TgConfig {
    fn api(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }
}

fn agent(timeout_secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout_secs))
        .build()
}

/// Core sender: posts any pre-formatted HTML text with Approve / Reject buttons.
/// Returns `(request_id, message_id)` — both needed for the polling phase.
fn send_with_approval(tg: &TgConfig, html: &str) -> Result<(String, i64)> {
    let uid = Uuid::new_v4().to_string();
    let request_id = uid[..8].to_string();

    let payload = serde_json::json!({
        "chat_id": tg.chat_id,
        "text": html,
        "parse_mode": "HTML",
        "reply_markup": {
            "inline_keyboard": [[
                {"text": "✅ Approve", "callback_data": format!("approve:{request_id}")},
                {"text": "❌ Reject",  "callback_data": format!("reject:{request_id}")}
            ]]
        }
    });

    let resp: serde_json::Value = agent(15)
        .post(&tg.api("sendMessage"))
        .set("Content-Type", "application/json")
        .send_json(&payload)
        .context("Failed to reach Telegram API")?
        .into_json()
        .context("Invalid Telegram response")?;

    if !resp["ok"].as_bool().unwrap_or(false) {
        return Err(anyhow!(
            "Telegram sendMessage failed: {}",
            resp["description"].as_str().unwrap_or("unknown error")
        ));
    }

    let message_id = resp["result"]["message_id"]
        .as_i64()
        .ok_or_else(|| anyhow!("Missing message_id in Telegram response"))?;

    Ok((request_id, message_id))
}

/// Format and send a PR approval notification.
pub fn send_approval_request(
    tg: &TgConfig,
    title: &str,
    body: &str,
    branch_info: &str,
    draft: bool,
) -> Result<(String, i64)> {
    let draft_badge = if draft { " · <b>DRAFT</b>" } else { "" };
    let body_section = {
        let trimmed = body.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n<b>Description</b>\n<pre>{}</pre>",
                escape_html(truncate(trimmed, 3000))
            )
        }
    };
    let html = format!(
        "🔀 <b>PR Review Required</b>{}\n\n<b>Title</b>   {}\n<b>Branch</b>  {}{}",
        draft_badge,
        escape_html(title),
        escape_html(branch_info),
        body_section,
    );
    send_with_approval(tg, &html)
}

/// Format and send a `gh api` mutation approval notification.
pub fn send_api_approval_request(
    tg: &TgConfig,
    method: &str,
    endpoint: Option<&str>,
    fields: &[String],
) -> Result<(String, i64)> {
    let endpoint_str = endpoint.unwrap_or("(unknown endpoint)");
    let mut html = format!(
        "🔧 <b>API Mutation · Approval Required</b>\n\n<code>{} {}</code>",
        escape_html(method),
        escape_html(endpoint_str),
    );
    if !fields.is_empty() {
        let formatted = fields
            .iter()
            .map(|f| {
                if let Some((k, v)) = f.split_once('=') {
                    format!("{} = {}", k, truncate(v, 300))
                } else {
                    f.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        html.push_str(&format!(
            "\n\n<b>Fields</b>\n<pre>{}</pre>",
            escape_html(&formatted)
        ));
    }
    send_with_approval(tg, &html)
}

/// Long-poll `getUpdates` until the user taps Approve or Reject, or we time out.
///
/// - Uses Telegram's server-side long-polling (up to 30 s per request) so we
///   get notified within ~1 s of the user tapping, with no busy-loop.
/// - After a decision the inline buttons are replaced with a status label so
///   the user can't accidentally double-tap.
pub fn poll_for_approval(
    tg: &TgConfig,
    request_id: &str,
    message_id: i64,
    timeout_secs: u64,
) -> Result<ApprovalResult> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    // HTTP timeout must exceed the Telegram long-poll window (30 s) plus overhead.
    let a = agent(45);
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

        // Ask Telegram to hold the connection for up to 30 s (or remaining time).
        let poll_timeout = remaining_secs.min(30);

        let mut req = serde_json::json!({
            "timeout": poll_timeout,
            "allowed_updates": ["callback_query"]
        });
        if let Some(off) = offset {
            req["offset"] = serde_json::json!(off);
        }

        match a
            .post(&tg.api("getUpdates"))
            .set("Content-Type", "application/json")
            .send_json(&req)
        {
            Ok(resp) => {
                let data: serde_json::Value = resp
                    .into_json()
                    .unwrap_or(serde_json::json!({"ok": false, "result": []}));

                if let Some(updates) = data["result"].as_array() {
                    for update in updates {
                        // Advance the offset so Telegram marks this update as seen.
                        let update_id = update["update_id"].as_i64().unwrap_or(0);
                        let next = update_id + 1;
                        offset = Some(offset.map_or(next, |prev| prev.max(next)));

                        let Some(cq) = update.get("callback_query") else {
                            continue;
                        };

                        let cb_data = cq["data"].as_str().unwrap_or("");

                        if cb_data == format!("approve:{request_id}") {
                            let _ = answer_callback(tg, cq, "✅ Approving…", &a);
                            let _ = replace_buttons(tg, message_id, "✅ Approved", &a);
                            return Ok(ApprovalResult::Approved);
                        }
                        if cb_data == format!("reject:{request_id}") {
                            let _ = answer_callback(tg, cq, "❌ Rejecting…", &a);
                            let _ = replace_buttons(tg, message_id, "❌ Rejected", &a);
                            return Ok(ApprovalResult::Rejected);
                        }
                        // Stale callback from a previous request — ack and discard.
                        let _ = answer_callback(tg, cq, "", &a);
                    }
                }
            }
            Err(e) => {
                eprintln!("  (Telegram poll error: {e} — retrying in 5 s…)");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }

    Ok(ApprovalResult::Timeout)
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Acknowledge a callback query, removing the loading spinner on the phone.
fn answer_callback(
    tg: &TgConfig,
    cq: &serde_json::Value,
    text: &str,
    a: &ureq::Agent,
) -> Result<()> {
    let id = cq["id"].as_str().unwrap_or("");
    a.post(&tg.api("answerCallbackQuery"))
        .set("Content-Type", "application/json")
        .send_json(&serde_json::json!({"callback_query_id": id, "text": text}))?;
    Ok(())
}

/// Swap the Approve/Reject buttons for a single non-actionable status label.
fn replace_buttons(
    tg: &TgConfig,
    message_id: i64,
    label: &str,
    a: &ureq::Agent,
) -> Result<()> {
    a.post(&tg.api("editMessageReplyMarkup"))
        .set("Content-Type", "application/json")
        .send_json(&serde_json::json!({
            "chat_id": tg.chat_id,
            "message_id": message_id,
            "reply_markup": {
                "inline_keyboard": [[{"text": label, "callback_data": "noop"}]]
            }
        }))?;
    Ok(())
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn truncate(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        s
    } else {
        let mut b = max_chars;
        while b > 0 && !s.is_char_boundary(b) {
            b -= 1;
        }
        &s[..b]
    }
}
