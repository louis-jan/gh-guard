/// Parsed metadata from a `gh api` invocation.
#[derive(Debug, Default)]
pub struct ApiArgs {
    /// HTTP method, always uppercase (e.g. "PATCH").
    pub method: String,
    /// The API endpoint positional argument (e.g. "/repos/owner/repo/pulls/123").
    pub endpoint: Option<String>,
    /// key=value pairs collected from --field / -f / --raw-field / -F.
    pub fields: Vec<String>,
    /// True for POST, PATCH, PUT, DELETE — the methods that mutate state.
    pub is_mutating: bool,
}

/// Scan raw `gh api` flags (everything after the "api" token) to extract
/// the method, endpoint, and fields we show in the approval notification.
/// Unknown flags are silently ignored; the original slice is always passed
/// through to `gh` unchanged after approval.
pub fn parse_api_args(args: &[String]) -> ApiArgs {
    let mut method = String::new();
    let mut endpoint: Option<String> = None;
    let mut fields: Vec<String> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].as_str();
        let next = || args.get(i + 1).map(String::as_str);

        match arg {
            "--method" | "-X" => {
                if let Some(v) = next() {
                    method = v.to_uppercase();
                    i += 2;
                    continue;
                }
            }
            "--field" | "-f" | "--raw-field" | "-F" => {
                if let Some(v) = next() {
                    fields.push(v.to_string());
                    i += 2;
                    continue;
                }
            }
            // Flags that consume a value but whose value we don't care about.
            "--header" | "-H" | "--jq" | "-q" | "--template" | "-t"
            | "--input" | "--cache" => {
                i += 2;
                continue;
            }
            _ => {
                // --flag=value forms.
                if let Some(v) = arg.strip_prefix("--method=") {
                    method = v.to_uppercase();
                } else if let Some(v) = arg.strip_prefix("--field=") {
                    fields.push(v.to_string());
                } else if let Some(v) = arg.strip_prefix("--raw-field=") {
                    fields.push(v.to_string());
                } else if !arg.starts_with('-') && endpoint.is_none() {
                    // First non-flag token is the endpoint.
                    endpoint = Some(arg.to_string());
                }
            }
        }
        i += 1;
    }

    // gh defaults to POST when --field flags are present, GET otherwise.
    if method.is_empty() {
        method = if fields.is_empty() {
            "GET".to_string()
        } else {
            "POST".to_string()
        };
    }

    let is_mutating = matches!(method.as_str(), "POST" | "PATCH" | "PUT" | "DELETE");

    ApiArgs { method, endpoint, fields, is_mutating }
}
