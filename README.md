# gh-guard 🪱

> *"How are you managing your PAT token right now?"*
> *"Sha!-hulud supply chain attacks scared the shit out of me. I don't want any agent touching my token."*

---

You gave your AI coding agent `GITHUB_TOKEN`. It has `repo` scope. It is running right now. You cannot see what it is doing.

The XZ backdoor took two years of patient, trusted commits before it struck. The next one won't announce itself either. It'll look like a helpful dependency update, a CI improvement, a perfectly normal `gh pr create`. And your PAT will be in the env.

> If you want to know what getting hit actually looks like — and how to harden your whole dev environment against it — read **[I Got Hit by Shai-Hulud: How I Rebuilt My Development Environment from the Ashes](https://dev.to/ottercyborg/i-got-hit-by-shai-hulud-how-i-rebuilt-my-development-environment-from-the-ashes-3ac2)**. gh-guard is one piece of that stack.

**gh-guard** is a wrapper around the GitHub CLI that keeps your PAT locked in the macOS Keychain — away from shell history, away from `.env` files, away from whatever is running in your terminal — and requires **your physical phone approval** before any PR or mutating API call goes through.

No agent creates a PR without you tapping a button. No `gh api --method PATCH` sneaks past. Your token never touches a config file.

---

## How it works

```
you type:  gh pr create --title "fix: auth" --body "..."
                │
                ▼
          gh-guard intercepts
                │
                ├── reads PAT from macOS Keychain (never from env/disk)
                │
                ├── sends rich Telegram notification to your phone:
                │
                │   ┌─────────────────────────────────────────┐
                │   │ 🔀 PR Review Required                   │
                │   │                                         │
                │   │ Title   fix: auth                       │
                │   │ Branch  feature/auth-fix → main         │
                │   │                                         │
                │   │ Description                             │
                │   │ ┌─────────────────────────────────────┐ │
                │   │ │ Fixes the token refresh on 401.     │ │
                │   │ │ Adds retry helper...                │ │
                │   │ └─────────────────────────────────────┘ │
                │   │                                         │
                │   │  [ ✅ Approve ]    [ ❌ Reject ]        │
                │   └─────────────────────────────────────────┘
                │
                ├── long-polls Telegram for your response
                │
                ├─── ✅ Approved  →  runs real `gh pr create`
                └─── ❌ Rejected  →  exits 1, nothing created
```

The same flow protects every `gh api` mutation (`PATCH`, `POST`, `PUT`, `DELETE`). Read-only calls pass through instantly.

---

## What is protected

| Command | Intercepted |
|---|---|
| `gh pr create --title "..."` | ✅ approval required |
| `gh api --method PATCH /repos/.../pulls/1` | ✅ approval required |
| `gh api --method DELETE /repos/.../labels/bug` | ✅ approval required |
| `gh api /repos/.../pulls` *(implicit POST with `--field`)* | ✅ approval required |
| `gh issue list` | ⏩ passthrough |
| `gh api /rate_limit` *(GET)* | ⏩ passthrough |
| `gh pr checkout 42` | ⏩ passthrough |

---

## Security model

- **PAT lives only in macOS Keychain.** It is never written to disk, never exported to the environment by you, never visible in shell history. gh-guard reads it at runtime and injects it as `GH_TOKEN` for the subprocess only.
- **Approval is on your phone.** Inline Telegram buttons are tied to a per-request UUID. A stale approval from a previous session cannot carry over.
- **The binary is not `gh`.** gh-guard is installed as `gh-guard` and aliased. When it calls the real `gh` after approval, it scans `$PATH` and skips its own resolved path to prevent loops. A `GH_GUARD_ACTIVE` env var provides a second layer.
- **Agents get nothing.** If an agent calls `gh`, it hits gh-guard. No title? No `--fill`? It gets an error. With `--title`? You get a notification. You approve or you don't.

---

## Installation

**Prerequisites**

- macOS (uses Keychain)
- [Rust](https://rustup.rs) (to build)
- [Telegram](https://telegram.org) app on your phone
- [GitHub CLI](https://cli.github.com) installed as the real `gh`

### Option A — Claude Code (recommended)

If you have [Claude Code](https://claude.ai/code) installed, the entire setup is one command:

```bash
git clone https://github.com/louis-jan/gh-guard
cd gh-guard
claude
```

Once Claude Code opens, run:

```
/setup
```

Claude will build the binary, install it, walk you through the GitHub PAT and Telegram bot prompts, send a test notification, and add the shell alias — all in one guided session.

<details>
<summary>Security concern: is my PAT exposed to Claude?</summary>

No. When Claude runs `gh-guard setup pat`, the PAT prompt uses
[`rpassword::prompt_password`](https://docs.rs/rpassword) which reads
directly from the TTY with echo disabled — bypassing stdout and stderr
entirely. Claude's Bash tool only captures stdout/stderr output; it
never has access to raw TTY input.

The only output Claude sees from the wizard is non-sensitive:

```
✓ (signed in as louis-jan)
PAT stored in macOS Keychain.
```

The same applies to the Telegram bot token prompt. And `gh-guard setup show`
only ever prints a masked value (`ghp_aPI…xs1Z`), so even credential
verification reveals nothing.

</details>

### Option B — Manual

**Build and install**

```bash
git clone https://github.com/louis-jan/gh-guard
cd gh-guard
cargo build --release
cp target/release/gh-guard ~/.local/bin/gh-guard
```

**Alias it**

```bash
# ~/.zshrc or ~/.bashrc
alias gh='gh-guard'
```

```bash
source ~/.zshrc
```

---

## Setup (manual)

```
gh-guard setup
```

The wizard walks you through two things:

**1. GitHub PAT**

Go to [github.com/settings/tokens](https://github.com/settings/tokens) and create a token with `repo` and `read:org` scopes. Or grab your current gh session token:

```bash
gh auth token | pbcopy
```

Paste it when prompted (input is hidden). gh-guard validates it against the GitHub API and stores it in the macOS Keychain — nowhere else.

**2. Telegram bot**

1. Open Telegram → search `@BotFather` → send `/newbot`
2. Follow the prompts, copy the token it gives you
3. Paste it when prompted (input is hidden)
4. gh-guard calls `getMe` to validate it, then asks you to send any message to your new bot
5. It auto-detects your chat ID from the incoming message and stores it in Keychain

That's it. Run `gh-guard setup test` to confirm your phone receives a message.

**Other setup subcommands**

```bash
gh-guard setup show      # show masked credentials from Keychain
gh-guard setup test      # send a test Telegram message
gh-guard setup pat       # update PAT only
gh-guard setup telegram  # update Telegram bot only
```

---

## Usage

Once the alias is set, use `gh` exactly as before. Everything passes through transparently — except mutations.

```bash
# Requires phone approval:
gh pr create --title "fix: token refresh" --body "Fixes #42"
gh pr create --fill
gh api --method PATCH /repos/org/repo/pulls/7 --field title="updated"
gh api --method DELETE /repos/org/repo/issues/3/labels/wontfix

# Passes through instantly:
gh pr list
gh issue view 42
gh repo clone org/repo
gh api /repos/org/repo/pulls
```

**Interactive `gh pr create` (no `--title`) is blocked** — gh-guard cannot intercept a TTY form, so it refuses and tells you to add `--title` or `--fill`.

**`--web` flag bypasses approval** — it opens a browser form rather than creating via API, so there is nothing to intercept.

---

## Why Telegram and not ntfy.sh

ntfy.sh notifications are plain text. A PR body with headers, code blocks, and bullet points looks like a wall of raw characters on your phone.

Telegram renders HTML natively in messages. Code blocks are syntax-highlighted, bold works, pre-formatted sections are scrollable. You can actually read the PR you're approving.

---

## Project layout

```
src/
├── main.rs      — dispatch, approval flows
├── config.rs    — macOS Keychain read/write
├── gh.rs        — find real gh binary, exec() passthrough
├── pr.rs        — parse gh pr create flags
├── api.rs       — parse gh api flags, detect mutating methods
├── notify.rs    — Telegram send + long-poll approval
└── setup.rs     — interactive setup wizard
```

---

## License

MIT
