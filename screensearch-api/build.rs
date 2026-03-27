//! Build script for screensearch-api
//!
//! This ensures the crate is rebuilt when embedded UI assets change.

fn main() {
    // Force rebuild when UI dist folder contents change
    // This is critical because rust-embed embeds files at compile time
    println!("cargo:rerun-if-changed=../screensearch-ui/dist");
    println!("cargo:rerun-if-changed=../screensearch-ui/dist/index.html");
    println!("cargo:rerun-if-changed=../screensearch-ui/dist/assets");
}
