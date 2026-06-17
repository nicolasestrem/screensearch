//! Build script for screensearch-api.
//!
//! Keeps the React bundle synchronized with the assets embedded by rust-embed.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is unavailable"),
    );
    let ui_dir = manifest_dir.join("../screensearch-ui");

    for path in [
        "src",
        "index.html",
        "package.json",
        "package-lock.json",
        "tsconfig.json",
        "tsconfig.node.json",
        "vite.config.ts",
        "tailwind.config.js",
        "postcss.config.js",
    ] {
        println!("cargo:rerun-if-changed={}", ui_dir.join(path).display());
    }
    println!("cargo:rerun-if-env-changed=SCREENSEARCH_SKIP_UI_BUILD");

    let skip_build = std::env::var("SCREENSEARCH_SKIP_UI_BUILD")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if skip_build {
        if !ui_dir.join("dist/index.html").exists() {
            panic!("SCREENSEARCH_SKIP_UI_BUILD is set but screensearch-ui/dist is missing");
        }
        return;
    }

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let status = Command::new(npm)
        .args(["run", "build"])
        .current_dir(&ui_dir)
        .status()
        .unwrap_or_else(|error| {
            panic!("failed to start the frontend build ({error}); run `npm ci` in screensearch-ui")
        });

    if !status.success() {
        panic!("frontend build failed; run `npm ci` and `npm run build` in screensearch-ui");
    }
}
