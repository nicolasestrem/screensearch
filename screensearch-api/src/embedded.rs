//! Embedded static assets for the web UI
//!
//! This module embeds all files from screensearch-ui/dist/ into the binary at compile time,
//! making the binary self-contained and portable.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../screensearch-ui/dist/"]
pub struct Assets;
