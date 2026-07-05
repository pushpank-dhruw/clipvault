use anyhow::Result;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep};

pub struct ClipboardMonitor {
    poll_interval: Duration,
    last_hash: Arc<Mutex<Option<String>>>,
}

impl ClipboardMonitor {
    pub fn new(poll_interval_ms: u64) -> Self {
        Self {
            poll_interval: Duration::from_millis(poll_interval_ms),
            last_hash: Arc::new(Mutex::new(None)),
        }
    }

    pub fn last_hash(&self) -> Arc<Mutex<Option<String>>> {
        self.last_hash.clone()
    }

    pub async fn run<F>(&self, mut on_change: F) -> Result<()>
    where
        F: FnMut(String, Option<String>),
    {
        loop {
            if let Ok(content) = get_clipboard_text().await {
                let hash = hex::encode(Sha256::digest(content.as_bytes()));
                let mut last = self.last_hash.lock().unwrap();
                if last.as_ref() != Some(&hash) {
                    tracing::debug!("clipboard changed: hash={}", &hash[..12]);
                    on_change(content.clone(), Some(hash.clone()));
                    *last = Some(hash);
                }
            }
            sleep(self.poll_interval).await;
        }
    }
}

async fn get_clipboard_text() -> Result<String> {
    let output = tokio::process::Command::new("wl-paste")
        .arg("--primary")
        .output()
        .await?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Ok(text);
        }
    }

    let output = tokio::process::Command::new("wl-paste").output().await?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Ok(text);
        }
    }

    Ok(String::new())
}
