//! `codescope upgrade` — in-place self-update from the latest
//! GitHub release. Mirrors what `install.sh`/`install.ps1` do at
//! first install, only it runs from inside an existing
//! installation so the user doesn't have to remember the pipe-
//! curl dance.
//!
//! Flow:
//! 1. Resolve the install directory (same rules as `install.rs`).
//! 2. Query `https://api.github.com/repos/<OWNER>/<REPO>/releases/latest`
//!    for the latest tag.
//! 3. Early-return "already on latest" when it matches this build's
//!    version.
//! 4. Download the right archive for the target triple + surreal
//!    binary bundle, unpack to a temp dir.
//! 5. Replace the on-disk binaries atomically (tmp-then-rename).
//! 6. Surreal binary lives at `~/.codescope/bin/surreal[.exe]` —
//!    drop the bundled one there.
//!
//! Network-shaped errors surface via the same R2 contract the CLI
//! uses elsewhere. `--yes` skips the confirmation prompt so CI /
//! cron can use this non-interactively.

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};

const REPO: &str = "onur-gokyildiz-bhi/codescope";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

pub async fn run(yes: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("  Current version: {current}");

    print!("  Checking latest release…");
    let _ = std::io::stdout().flush();

    let latest = fetch_latest_tag().await.context("query GitHub releases")?;
    println!(" {latest}");
    let latest_no_v = latest.strip_prefix('v').unwrap_or(&latest);
    if latest_no_v == current {
        println!("  Already on latest.");
        return Ok(());
    }
    println!("  Upgrade available: {current} → {latest_no_v}");

    if !yes && !confirm("Proceed? [y/N] ")? {
        println!("  aborted.");
        return Ok(());
    }

    let triple = target_triple()?;
    let archive_name = archive_name_for(&triple, &latest);
    let url = format!("https://github.com/{REPO}/releases/download/{latest}/{archive_name}");
    println!("  Downloading {archive_name}…");

    let tmp_dir = std::env::temp_dir().join(format!("codescope-upgrade-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).ok();
    let archive_path = tmp_dir.join(&archive_name);
    download(&url, &archive_path)
        .await
        .with_context(|| format!("download {url}"))?;

    let extract_dir = tmp_dir.join("unpack");
    std::fs::create_dir_all(&extract_dir).ok();
    unpack(&archive_path, &extract_dir)?;

    let install_dir = install_dir();
    std::fs::create_dir_all(&install_dir).ok();
    let surreal_dir = surreal_bin_dir();
    std::fs::create_dir_all(&surreal_dir).ok();

    // Copy every expected binary. `codescope-lsp` is optional —
    // older releases didn't ship it.
    let mut moved = Vec::new();
    for name in [
        "codescope",
        "codescope-mcp",
        "codescope-web",
        "codescope-lsp",
    ] {
        let exe = exe_name(name);
        let src = find_in_tree(&extract_dir, &exe);
        if let Some(src) = src {
            let dst = install_dir.join(&exe);
            replace_file(&src, &dst).with_context(|| format!("install {exe}"))?;
            moved.push(exe);
        }
    }
    // surreal — drop under ~/.codescope/bin/ only (not the generic
    // install dir) so it doesn't pollute PATH and so the R4
    // supervisor finds it at the documented location.
    let surreal_exe = if cfg!(windows) {
        "surreal.exe"
    } else {
        "surreal"
    };
    if let Some(src) = find_in_tree(&extract_dir, surreal_exe) {
        let dst = surreal_dir.join(surreal_exe);
        replace_file(&src, &dst).with_context(|| format!("install {surreal_exe}"))?;
        moved.push(format!("{surreal_exe} → {}", surreal_dir.display()));
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);

    println!();
    println!("  ✓ Upgraded to {latest_no_v}");
    for m in &moved {
        println!("    · {m}");
    }
    println!();
    println!("  Restart running codescope processes to pick up the new binary:");
    println!("    codescope stop && codescope start");
    println!();
    Ok(())
}

async fn fetch_latest_tag() -> Result<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent(format!("codescope/{} upgrade", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(&url).send().await?.error_for_status()?;
    let rel: Release = resp.json().await?;
    Ok(rel.tag_name)
}

async fn download(url: &str, dest: &Path) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(format!("codescope/{} upgrade", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    let bytes = resp.bytes().await?;
    std::fs::write(dest, &bytes)?;
    Ok(())
}

fn unpack(archive: &Path, out: &Path) -> Result<()> {
    let name = archive
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if name.ends_with(".zip") {
        // Windows archives use zip. Shell out to `tar` which now
        // ships with Windows 10+ and handles zip natively. Avoids
        // pulling a zip crate just for this one call.
        let status = std::process::Command::new("tar")
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(out)
            .status()
            .context("spawn tar")?;
        if !status.success() {
            bail!("tar unzip exited {status}");
        }
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let status = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(archive)
            .arg("-C")
            .arg(out)
            .status()
            .context("spawn tar")?;
        if !status.success() {
            bail!("tar gunzip exited {status}");
        }
    } else {
        bail!("unsupported archive: {name}");
    }
    Ok(())
}

/// Replace an existing binary with a new one. On Windows you can't
/// overwrite an in-use .exe — rename it aside first, then write
/// the new file. The stale `.old` gets deleted on next upgrade.
fn replace_file(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        let aside = dst.with_extension("old");
        let _ = std::fs::remove_file(&aside);
        std::fs::rename(dst, &aside).ok();
    }
    std::fs::copy(src, dst)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dst)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(dst, perms)?;
    }
    Ok(())
}

fn find_in_tree(root: &Path, name: &str) -> Option<PathBuf> {
    for entry in walk(root) {
        if entry.file_name().and_then(|s| s.to_str()) == Some(name) {
            return Some(entry);
        }
    }
    None
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&p) else {
            continue;
        };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

fn install_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("AppData")
                    .join("Local")
            })
            .join("codescope")
            .join("bin")
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local")
            .join("bin")
    }
}

fn surreal_bin_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("bin")
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// Archive names mirror release.yml.
fn archive_name_for(triple: &str, tag: &str) -> String {
    if triple.contains("windows") {
        format!("codescope-{tag}-{triple}.zip")
    } else {
        format!("codescope-{tag}-{triple}.tar.gz")
    }
}

fn target_triple() -> Result<String> {
    // Matches the `release.yml` matrix. The host-triple crate would
    // do this too — but we want zero extra deps here.
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let triple = match (os, arch) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => return Err(anyhow!("no prebuilt binaries for {os}/{arch}")),
    };
    Ok(triple.to_string())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("  {prompt}");
    std::io::stdout().flush().ok();
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).ok();
    let ans = buf.trim().to_ascii_lowercase();
    Ok(ans == "y" || ans == "yes")
}
