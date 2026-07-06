//! Optical character recognition via the `tesseract` CLI.
//!
//! The shelf's OCR button extracts text from the selected image entry. This
//! shells out to `tesseract` (already the standard Linux OCR engine) rather
//! than linking a native library, keeping the daemon dependency-free.

use anyhow::{Context, Result, bail};
use std::process::Command;

/// Whether the `tesseract` binary is on `PATH`.
pub fn available() -> bool {
    Command::new("tesseract")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Recognize text in `image` (encoded bytes, e.g. PNG). Returns the trimmed
/// text, which may be empty when the image contains no legible text.
pub fn recognize(id: i64, image: &[u8]) -> Result<String> {
    let path =
        std::env::temp_dir().join(format!("clipvault-ocr-{}-{}.png", std::process::id(), id));
    std::fs::write(&path, image).context("failed to write OCR temp file")?;

    let output = Command::new("tesseract").arg(&path).arg("stdout").output();
    let _ = std::fs::remove_file(&path);

    let output = output.context("failed to run tesseract")?;
    if !output.status.success() {
        bail!("tesseract exited with {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
