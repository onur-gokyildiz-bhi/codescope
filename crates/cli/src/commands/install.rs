use anyhow::Result;
use std::path::PathBuf;

pub fn run() -> Result<()> {
    // Find the compiled binary
    let exe_name = if cfg!(windows) {
        "codescope-mcp.exe"
    } else {
        "codescope-mcp"
    };
    let cli_exe = if cfg!(windows) {
        "codescope.exe"
    } else {
        "codescope"
    };
    let web_exe = if cfg!(windows) {
        "codescope-web.exe"
    } else {
        "codescope-web"
    };

    // Try to find from same directory as current executable, or from target/release
    let current_exe = std::env::current_exe().ok();
    let source_dir = current_exe
        .as_ref()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf());

    let install_dir = if cfg!(windows) {
        // Match install.ps1: %LOCALAPPDATA%\codescope\bin
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
    };

    std::fs::create_dir_all(&install_dir)?;

    let mut installed = Vec::new();

    for binary in &[exe_name, cli_exe, web_exe] {
        let source = source_dir.as_ref().map(|d| d.join(binary));
        if let Some(src) = &source {
            if src.exists() {
                let dest = install_dir.join(binary);
                std::fs::copy(src, &dest)?;
                installed.push(dest.display().to_string());
            }
        }
    }

    if installed.is_empty() {
        println!("⚠ No binaries found. Build first:\n");
        println!("  cargo build --release");
        println!("\nThen run from the release directory:");
        println!("  ./target/release/codescope install");
        return Ok(());
    }

    println!(
        "✅ Installed {} binaries to {}:\n",
        installed.len(),
        install_dir.display()
    );
    for p in &installed {
        println!("   {}", p);
    }

    // Check if install_dir is in PATH
    let path_var = std::env::var("PATH").unwrap_or_default();
    let install_str = install_dir.to_string_lossy();
    if !path_var.contains(install_str.as_ref()) {
        println!("\n⚠ {} is not in your PATH. Add it:", install_dir.display());
        if cfg!(windows) {
            println!("\n  PowerShell (permanent):");
            println!(
                "  [Environment]::SetEnvironmentVariable('PATH', $env:PATH + ';{}', 'User')",
                install_dir.display()
            );
        } else {
            println!(
                "\n  echo 'export PATH=\"{}:$PATH\"' >> ~/.bashrc && source ~/.bashrc",
                install_dir.display()
            );
        }
    }

    println!("\n🚀 Now run in any project:");
    println!("   cd <your-project>");
    println!("   codescope init");

    Ok(())
}
