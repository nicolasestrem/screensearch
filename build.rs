use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=screensearch-ui/src");
    println!("cargo:rerun-if-changed=screensearch-ui/package.json");
    println!("cargo:rerun-if-env-changed=SKIP_UI_BUILD");

    // Check if UI build should be skipped (useful for faster iteration during development)
    let skip_ui_build = env::var("SKIP_UI_BUILD").is_ok();
    if skip_ui_build {
        println!("cargo:warning=Skipping UI build (SKIP_UI_BUILD environment variable set)");
        println!("cargo:warning=Note: Application will serve 'UI not built' page. Build UI manually with: cd screensearch-ui && npm run build");
        return;
    }

    // Only build UI in release mode to speed up dev builds
    let profile = env::var("PROFILE").unwrap_or_default();

    if profile == "release" {
        println!("cargo:warning=Building web UI...");

        // Detect cross-compilation: when building on Linux for Windows target
        // CARGO_CFG_TARGET_OS contains the target OS, not the host OS
        let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        let is_cross_compiling = target_os == "windows" && cfg!(target_os = "linux");

        // Check if npm is available
        // IMPORTANT: Use host OS commands (where npm actually runs), not target OS
        // When cross-compiling from Linux to Windows, npm runs on Linux, not Windows
        let npm_check = if is_cross_compiling || cfg!(target_os = "linux") {
            Command::new("which").arg("npm").output()
        } else if cfg!(target_os = "windows") {
            Command::new("where").arg("npm").output()
        } else {
            // Fallback for other Unix-like systems
            Command::new("which").arg("npm").output()
        };

        if npm_check.is_ok() && npm_check.unwrap().status.success() {
            println!("cargo:warning=npm found, building web UI...");

            // Run npm install using current_dir() instead of shell cd
            // On Windows, npm is actually npm.cmd
            // When cross-compiling, use the host OS npm command
            let npm_cmd = if is_cross_compiling || cfg!(target_os = "linux") {
                "npm"  // Linux/Unix systems use plain "npm"
            } else if cfg!(target_os = "windows") {
                "npm.cmd"  // Native Windows builds use "npm.cmd"
            } else {
                "npm"  // Fallback for other Unix-like systems
            };

            let install_status = Command::new(npm_cmd)
                .arg("install")
                .current_dir("screensearch-ui")
                .status();

            match install_status {
                Ok(status) if status.success() => {
                    println!("cargo:warning=npm install completed successfully");
                }
                Ok(status) => {
                    panic!("npm install failed with exit code: {:?}. Check screensearch-ui/package.json or run manually: cd screensearch-ui && npm install", status.code());
                }
                Err(e) => {
                    panic!(
                        "Failed to execute npm install: {}. Install Node.js or build UI manually.",
                        e
                    );
                }
            }

            // Run npm run build using current_dir() instead of shell cd
            let build_status = Command::new(npm_cmd)
                .arg("run")
                .arg("build")
                .current_dir("screensearch-ui")
                .status();

            match build_status {
                Ok(status) if status.success() => {
                    println!("cargo:warning=Web UI built successfully!");
                }
                Ok(status) => {
                    panic!("npm run build failed with exit code: {:?}. Run manually: cd screensearch-ui && npm run build", status.code());
                }
                Err(e) => {
                    panic!("Failed to execute npm run build: {}. Install Node.js or build UI manually.", e);
                }
            }
        } else {
            println!("cargo:warning=npm not found. Skipping UI build. Install Node.js or build UI manually.");
        }
    }
}
