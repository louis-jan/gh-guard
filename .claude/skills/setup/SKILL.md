---
name: setup
description: Build, install, and configure gh-guard end-to-end. Use when the user wants to set up gh-guard for the first time, reinstall after changes, or reconfigure credentials.
argument-hint: "[build | install | credentials | all]"
disable-model-invocation: true
allowed-tools: Bash(cargo *), Bash(cp *), Bash(mkdir *), Bash(git *)
---

Walk the user through every step of getting gh-guard fully operational.
Infer which phase to start from based on `$ARGUMENTS` or the current state:

- `build`       → compile only
- `install`     → compile + copy binary
- `credentials` → run the setup wizard only (binary already installed)
- `all` or none → full flow below

---

## Phase 1 — Build

Check whether a fresh build is needed:

```bash
ls target/release/gh-guard 2>/dev/null && echo "exists" || echo "missing"
```

If missing or the user explicitly requested a build, compile the release binary:

```bash
cargo build --release
```

Surface any compiler errors clearly. Do not proceed to Phase 2 if the build fails.

---

## Phase 2 — Install

Confirm the install target. Prefer `~/.local/bin` if it is already in `$PATH`:

```bash
printenv PATH | tr ':' '\n' | grep -E "local/bin|\.bin"
```

Copy the binary:

```bash
cp target/release/gh-guard ~/.local/bin/gh-guard
```

Verify the installed binary responds:

```bash
gh-guard setup show
```

If `gh-guard` is not found after copying, diagnose whether `~/.local/bin` is in `$PATH` and tell the user exactly which line to add to their shell config.

---

## Phase 3 — Credentials

Tell the user what will be stored before asking for anything:

> gh-guard stores two secrets in macOS Keychain — your GitHub PAT and a Telegram bot token + chat ID. Nothing is written to disk or env files.

### 3a. GitHub PAT

Run the PAT sub-wizard:

```bash
gh-guard setup pat
```

If the user doesn't have a PAT ready, remind them:
- Create one at https://github.com/settings/tokens (scopes: `repo`, `read:org`)
- Or copy the current gh session token: `gh auth token | pbcopy`

### 3b. Telegram bot

Run the Telegram sub-wizard:

```bash
gh-guard setup telegram
```

Guide the user through BotFather if they haven't created a bot yet:
1. Open Telegram → search `@BotFather`
2. Send `/newbot`, pick a name and username
3. Copy the token BotFather gives them
4. Paste it into the prompt (input is hidden)
5. Send any message to the new bot in Telegram — gh-guard auto-detects the chat ID

---

## Phase 4 — Verify

Send a test notification to confirm the full chain works:

```bash
gh-guard setup test
```

If the test message doesn't arrive, help diagnose:
- Run `gh-guard setup show` to confirm both credentials are stored
- Check that the Telegram bot isn't blocked or muted
- Confirm the user is subscribed to their own bot (they need to have messaged it at least once)

---

## Phase 5 — Shell alias

Check whether the alias is already set:

```bash
grep "alias gh=" ~/.zshrc ~/.bashrc 2>/dev/null
```

If missing, show the user the exact line to add and which file to put it in based on their shell:

```bash
# ~/.zshrc  (zsh)
alias gh='gh-guard'
```

Then reload:

```bash
source ~/.zshrc   # or ~/.bashrc
```

---

## Done

Confirm the setup is complete with a summary of what was configured.
Remind the user: `gh pr create --title "..."` and `gh api --method PATCH ...` will now require phone approval before anything hits GitHub.
