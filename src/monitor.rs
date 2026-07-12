use crate::config::Config;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep};

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Image { data: Vec<u8>, mime_type: String },
}

impl ClipboardContent {
    pub fn hash(&self) -> String {
        match self {
            ClipboardContent::Text(s) => hex::encode(Sha256::digest(s.as_bytes())),
            ClipboardContent::Image { data, .. } => hex::encode(Sha256::digest(data)),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            ClipboardContent::Text(s) => s.len(),
            ClipboardContent::Image { data, .. } => data.len(),
        }
    }
}

pub struct ClipboardMonitor {
    config: Arc<Mutex<Config>>,
    last_hash: Arc<Mutex<Option<String>>>,
}

impl ClipboardMonitor {
    pub fn new(config: Arc<Mutex<Config>>) -> Self {
        Self {
            config,
            last_hash: Arc::new(Mutex::new(None)),
        }
    }

    pub fn last_hash(&self) -> Arc<Mutex<Option<String>>> {
        self.last_hash.clone()
    }

    pub async fn run<F>(&self, mut on_change: F) -> Result<()>
    where
        F: FnMut(ClipboardContent, Option<String>),
    {
        loop {
            // Read poll interval and the sensitive-capture policy fresh each
            // tick so the settings window can change them live.
            let (poll_ms, hide_sensitive) = {
                let c = self.config.lock().unwrap();
                (c.poll_interval_ms.max(50), c.hide_sensitive)
            };
            if let Some(content) = get_clipboard_content(hide_sensitive).await {
                let hash = content.hash();
                let mut last = self.last_hash.lock().unwrap();
                if last.as_ref() != Some(&hash) {
                    let size = content.size();
                    tracing::debug!(
                        "clipboard changed: type={} hash={} size={}",
                        match &content {
                            ClipboardContent::Text(_) => "text",
                            ClipboardContent::Image { .. } => "image",
                        },
                        &hash[..12],
                        size
                    );
                    on_change(content, Some(hash.clone()));
                    *last = Some(hash);
                }
            }
            sleep(Duration::from_millis(poll_ms)).await;
        }
    }
}

/// Whether the current clipboard offer is marked sensitive (e.g. a password
/// manager sets `x-kde-passwordManagerHint`), so it should not be recorded.
fn is_sensitive(mime_types: &[String]) -> bool {
    mime_types.iter().any(|t| {
        let t = t.to_ascii_lowercase();
        t.contains("x-kde-passwordmanagerhint")
            || t.contains("sensitive")
            || t.contains("concealed")
    })
}

async fn get_clipboard_content(hide_sensitive: bool) -> Option<ClipboardContent> {
    let mime_types = get_available_mime_types().await;

    if hide_sensitive && is_sensitive(&mime_types) {
        return None;
    }

    if let Some(mime) = mime_types.iter().find(|t| t.starts_with("image/"))
        && let Some(data) = capture_image(mime).await
    {
        return Some(ClipboardContent::Image {
            data,
            mime_type: mime.clone(),
        });
    }

    if mime_types
        .iter()
        .any(|t| t == "text/plain" || t == "text/plain;charset=utf-8")
        || mime_types.is_empty()
    {
        if let Some(text) = capture_text(false).await
            && !text.is_empty()
        {
            return Some(ClipboardContent::Text(text));
        }
        if let Some(text) = capture_text(true).await
            && !text.is_empty()
        {
            return Some(ClipboardContent::Text(text));
        }
    }

    None
}

async fn get_available_mime_types() -> Vec<String> {
    let output = tokio::process::Command::new("wl-paste")
        .arg("--list-types")
        .output()
        .await
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .unwrap_or_default();
    output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

async fn capture_image(mime_type: &str) -> Option<Vec<u8>> {
    let output = tokio::process::Command::new("wl-paste")
        .arg("--type")
        .arg(mime_type)
        .output()
        .await
        .ok()?;
    if output.status.success() && !output.stdout.is_empty() {
        Some(output.stdout)
    } else {
        None
    }
}

async fn capture_text(primary: bool) -> Option<String> {
    let mut cmd = tokio::process::Command::new("wl-paste");
    if primary {
        cmd.arg("--primary");
    }
    let output = cmd.output().await.ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}
