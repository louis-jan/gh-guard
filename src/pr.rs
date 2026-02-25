/// Parsed metadata extracted from raw `gh pr create` flags.
/// Unknown flags are ignored here — the original slice is always passed
/// through to `gh` unchanged so nothing is lost.
#[derive(Debug, Default)]
pub struct PrArgs {
    pub title: Option<String>,
    pub body: Option<String>,
    pub body_file: Option<String>,
    pub base: Option<String>,
    pub draft: bool,
    pub fill: bool,
    pub web: bool,
    /// True only when --title was explicitly supplied.
    pub has_title: bool,
}

/// Scan the raw flag slice for known `gh pr create` options.
pub fn parse_pr_args(args: &[String]) -> PrArgs {
    let mut out = PrArgs::default();
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].as_str();

        // Helper: peek at the next token as a value.
        let next = || args.get(i + 1).map(String::as_str);

        match arg {
            "--title" | "-t" => {
                if let Some(v) = next() {
                    out.title = Some(v.to_string());
                    out.has_title = true;
                    i += 2;
                    continue;
                }
            }
            "--body" | "-b" => {
                if let Some(v) = next() {
                    out.body = Some(v.to_string());
                    i += 2;
                    continue;
                }
            }
            "--body-file" | "-F" => {
                if let Some(v) = next() {
                    out.body_file = Some(v.to_string());
                    i += 2;
                    continue;
                }
            }
            "--base" | "-B" => {
                if let Some(v) = next() {
                    out.base = Some(v.to_string());
                    i += 2;
                    continue;
                }
            }
            "--draft" | "-d" => {
                out.draft = true;
            }
            "--fill" | "--fill-verbose" => {
                out.fill = true;
            }
            "--web" | "-w" => {
                out.web = true;
            }
            _ => {
                // Handle --flag=value forms.
                if let Some(v) = arg.strip_prefix("--title=") {
                    out.title = Some(v.to_string());
                    out.has_title = true;
                } else if let Some(v) = arg.strip_prefix("--body=") {
                    out.body = Some(v.to_string());
                } else if let Some(v) = arg.strip_prefix("--base=") {
                    out.base = Some(v.to_string());
                } else if let Some(v) = arg.strip_prefix("--body-file=") {
                    out.body_file = Some(v.to_string());
                }
            }
        }
        i += 1;
    }
    out
}

/// Return body text: inline --body takes priority, then --body-file.
pub fn resolve_body(pr: &PrArgs) -> String {
    if let Some(ref body) = pr.body {
        return body.clone();
    }
    if let Some(ref path) = pr.body_file {
        return std::fs::read_to_string(path).unwrap_or_default();
    }
    String::new()
}

/// Human-readable "source → base" branch string shown in the notification.
pub fn branch_info(base: Option<&str>) -> String {
    let current = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "HEAD");

    match (current, base) {
        (Some(curr), Some(b)) => format!("{curr} → {b}"),
        (Some(curr), None) => format!("{curr} → (default branch)"),
        (None, Some(b)) => format!("(current) → {b}"),
        (None, None) => "(current) → (default branch)".to_string(),
    }
}
