use std::process::Command;

fn main() {
    // Get short hash (7 chars) for display
    let short_output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output();

    let git_hash_short = match short_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };

    // Get full hash for hover text
    let full_output = Command::new("git").args(["rev-parse", "HEAD"]).output();

    let git_hash_full = match full_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let rustc_version = match Command::new(rustc).arg("--version").output() {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .trim()
            .strip_prefix("rustc ")
            .unwrap_or("")
            .to_string(),
        _ => "unknown".to_string(),
    };

    // In a git worktree `.git` is a file, so `../../.git/HEAD` does not exist and cargo would
    // rerun this script, and rebuild the crate, on every build. Ask git for the real paths.
    if let (Some(head), Some(common)) = (
        git_path(&["--git-path", "HEAD"]),
        git_path(&["--git-common-dir"]),
    ) {
        println!("cargo::rerun-if-changed={head}");
        println!("cargo::rerun-if-changed={common}/refs/heads/");
    }
    println!("cargo::rustc-env=GIT_HASH={git_hash_short}");
    println!("cargo::rustc-env=GIT_HASH_FULL={git_hash_full}");
    println!("cargo::rustc-env=RUSTC_VERSION={rustc_version}");
}

fn git_path(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--path-format=absolute"])
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
