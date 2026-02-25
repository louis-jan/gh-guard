use anyhow::{bail, Result};
use std::path::PathBuf;
use std::process;

/// Search PATH for the real `gh` binary, skipping our own executable.
/// This prevents an infinite loop when gh-guard is installed as 'gh'.
pub fn find_real_gh() -> Result<PathBuf> {
    let self_exe = std::env::current_exe()?;
    let self_resolved = self_exe.canonicalize().unwrap_or(self_exe);

    let path_var = std::env::var("PATH").unwrap_or_default();

    for dir in path_var.split(':') {
        let candidate = PathBuf::from(dir).join("gh");
        if !candidate.exists() {
            continue;
        }
        // Resolve symlinks before comparing so a symlink-as-gh is detected.
        let resolved = candidate.canonicalize().unwrap_or_else(|_| candidate.clone());
        if resolved == self_resolved {
            continue; // skip ourselves
        }
        // Verify the file is executable.
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = candidate.metadata() {
            if meta.permissions().mode() & 0o111 != 0 {
                return Ok(candidate);
            }
        }
    }

    bail!(
        "Could not find the real `gh` binary in PATH.\n\
         Install the GitHub CLI: https://cli.github.com"
    )
}

/// Replace the current process with `gh <args>` using exec(2).
/// On success this never returns; on failure it returns an error.
/// Using exec() preserves TTY ownership and correct signal delivery.
pub fn exec_passthrough(args: &[String], token: Option<&str>) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let gh = find_real_gh()?;
    let mut cmd = process::Command::new(&gh);
    cmd.args(args).env("GH_GUARD_ACTIVE", "1");
    if let Some(t) = token {
        cmd.env("GH_TOKEN", t);
    }
    // exec() only returns on failure.
    let err = cmd.exec();
    bail!("Failed to exec {}: {}", gh.display(), err)
}

/// Spawn `gh <args>` as a child process and return its exit code.
/// Used post-approval so we can capture the code and exit cleanly.
pub fn run_gh(args: &[String], token: Option<&str>) -> Result<i32> {
    let gh = find_real_gh()?;
    let mut cmd = process::Command::new(&gh);
    cmd.args(args).env("GH_GUARD_ACTIVE", "1");
    if let Some(t) = token {
        cmd.env("GH_TOKEN", t);
    }
    let status = cmd.status()?;
    Ok(status.code().unwrap_or(1))
}
