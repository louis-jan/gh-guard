mod api;
mod config;
mod gh;
mod notify;
mod pr;
mod setup;

use anyhow::{bail, Result};
use colored::Colorize;
use notify::ApprovalResult;
use std::process;

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "gh-guard error:".red().bold(), e);
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // ── Infinite-loop guard ──────────────────────────────────────────────────
    // If gh-guard is installed as 'gh' (symlink / PATH shadow) and we call
    // the real gh after approval, we set GH_GUARD_ACTIVE so a re-entered
    // instance skips interception and goes straight to passthrough.
    if std::env::var("GH_GUARD_ACTIVE").is_ok() {
        let token = config::get_pat().ok();
        let code = gh::run_gh(&args, token.as_deref())?;
        process::exit(code);
    }

    match args.first().map(String::as_str) {
        // No args: hand off to gh (shows gh's own help)
        None => passthrough(&args),

        // Built-in setup wizard
        Some("setup") => setup::run(args.get(1).map(String::as_str)),

        // PR creation with phone approval
        Some("pr") if args.get(1).map(String::as_str) == Some("create") => {
            let pr_flags: &[String] = if args.len() > 2 { &args[2..] } else { &[] };
            handle_pr_create(pr_flags)
        }

        // gh api mutations (PATCH, POST, PUT, DELETE) with phone approval
        Some("api") => {
            let api_flags: &[String] = if args.len() > 1 { &args[1..] } else { &[] };
            handle_api(api_flags)
        }

        // Everything else: transparent passthrough
        _ => passthrough(&args),
    }
}

fn handle_pr_create(raw_flags: &[String]) -> Result<()> {
    let parsed = pr::parse_pr_args(raw_flags);

    // --web opens a browser form; no meaningful interception possible.
    if parsed.web {
        eprintln!("{}", "gh-guard: --web flag detected, bypassing approval flow.".yellow());
        let mut full = vec!["pr".to_string(), "create".to_string()];
        full.extend_from_slice(raw_flags);
        return passthrough(&full);
    }

    // Require --title or --fill so the notification has useful content.
    // Without them gh would open an interactive TUI we cannot intercept.
    if !parsed.has_title && !parsed.fill {
        bail!(
            "gh-guard cannot intercept interactive PR creation.\n\
             Add --title \"...\" (and optionally --body \"...\") to your command,\n\
             or use --fill to auto-fill from commit messages."
        );
    }

    let token = config::get_pat()?;
    let tg = notify::TgConfig {
        token: config::get_telegram_token()?,
        chat_id: config::get_telegram_chat_id()?,
    };

    let body_text = pr::resolve_body(&parsed);
    let pr_title = parsed
        .title
        .as_deref()
        .unwrap_or("(auto-fill from commits)");
    let branch_info = pr::branch_info(parsed.base.as_deref());

    eprintln!("{}", "══════════════════════════════════".cyan());
    eprintln!("{}", " gh-guard · PR Approval Required  ".cyan().bold());
    eprintln!("{}", "══════════════════════════════════".cyan());
    eprintln!("  Title  : {}", pr_title.bold());
    eprintln!("  Branch : {}", branch_info);
    if parsed.draft {
        eprintln!("  Mode   : {}", "draft".yellow());
    }
    eprintln!();
    eprintln!("Sending to Telegram…");

    let (request_id, message_id) = notify::send_approval_request(
        &tg,
        pr_title,
        &body_text,
        &branch_info,
        parsed.draft,
    )?;

    eprintln!("Waiting for approval on Telegram (5-min timeout)…");

    match notify::poll_for_approval(&tg, &request_id, message_id, 300)? {
        ApprovalResult::Approved => {
            eprintln!("{}", "✅  Approved! Creating PR…".green().bold());
            let mut full_args = vec!["pr".to_string(), "create".to_string()];
            full_args.extend_from_slice(raw_flags);
            let code = gh::run_gh(&full_args, Some(&token))?;
            process::exit(code);
        }
        ApprovalResult::Rejected => {
            eprintln!("{}", "❌  Rejected. PR not created.".red().bold());
            process::exit(1);
        }
        ApprovalResult::Timeout => {
            eprintln!("{}", "⏱   Timed out (5 min). PR not created.".yellow());
            process::exit(1);
        }
    }
}

fn handle_api(api_flags: &[String]) -> Result<()> {
    let parsed = api::parse_api_args(api_flags);

    // GET / HEAD are read-only — pass straight through.
    if !parsed.is_mutating {
        let mut full = vec!["api".to_string()];
        full.extend_from_slice(api_flags);
        return passthrough(&full);
    }

    let token = config::get_pat()?;
    let tg = notify::TgConfig {
        token: config::get_telegram_token()?,
        chat_id: config::get_telegram_chat_id()?,
    };

    let endpoint_display = parsed.endpoint.as_deref().unwrap_or("(unknown)");

    eprintln!("{}", "══════════════════════════════════".cyan());
    eprintln!("{}", " gh-guard · API Approval Required ".cyan().bold());
    eprintln!("{}", "══════════════════════════════════".cyan());
    eprintln!("  Method   : {}", parsed.method.yellow().bold());
    eprintln!("  Endpoint : {}", endpoint_display);
    for f in &parsed.fields {
        eprintln!("    {}", f.dimmed());
    }
    eprintln!();
    eprintln!("Sending to Telegram…");

    let (request_id, message_id) = notify::send_api_approval_request(
        &tg,
        &parsed.method,
        parsed.endpoint.as_deref(),
        &parsed.fields,
    )?;

    eprintln!("Waiting for approval on Telegram (5-min timeout)…");

    match notify::poll_for_approval(&tg, &request_id, message_id, 300)? {
        ApprovalResult::Approved => {
            eprintln!("{}", "✅  Approved! Running API call…".green().bold());
            let mut full = vec!["api".to_string()];
            full.extend_from_slice(api_flags);
            let code = gh::run_gh(&full, Some(&token))?;
            process::exit(code);
        }
        ApprovalResult::Rejected => {
            eprintln!("{}", "❌  Rejected. API call cancelled.".red().bold());
            process::exit(1);
        }
        ApprovalResult::Timeout => {
            eprintln!("{}", "⏱   Timed out (5 min). API call cancelled.".yellow());
            process::exit(1);
        }
    }
}

/// Replace the current process with `gh <args>`, injecting GH_TOKEN.
/// Uses exec() on Unix so TTY ownership and signal handling are correct.
fn passthrough(args: &[String]) -> Result<()> {
    let token = config::get_pat().ok();
    gh::exec_passthrough(args, token.as_deref())
}
