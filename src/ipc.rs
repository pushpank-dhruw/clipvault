use anyhow::{Context, Result};
use interprocess::local_socket::traits::ListenerExt;
use interprocess::local_socket::{ConnectOptions, GenericFilePath, ListenerOptions, ToFsName};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const TOGGLE_CMD: &[u8] = b"toggle";
pub const QUIT_CMD: &[u8] = b"quit";
pub const STATUS_CMD: &[u8] = b"status";

pub fn send_command(socket_path: &Path, cmd: &[u8]) -> Result<String> {
    if !socket_path.exists() {
        return Err(anyhow::anyhow!("daemon not running (socket not found)"));
    }
    let name = socket_path
        .to_fs_name::<GenericFilePath>()
        .context("failed to create socket name")?;
    let mut conn = ConnectOptions::new()
        .name(name)
        .connect_sync()
        .context("failed to connect to daemon socket")?;

    conn.write_all(cmd).context("failed to send command")?;
    conn.flush()?;

    let mut response = String::new();
    conn.read_to_string(&mut response).ok();
    Ok(response)
}

pub fn listen(socket_path: &Path, toggle_flag: Arc<AtomicBool>) -> Result<()> {
    let _ = std::fs::remove_file(socket_path);

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let name = socket_path
        .to_fs_name::<GenericFilePath>()
        .context("failed to create socket name")?;
    let listener = ListenerOptions::new()
        .name(name)
        .create_sync()
        .context("failed to create IPC listener")?;

    tracing::info!("IPC listener on {}", socket_path.display());

    for conn in listener.incoming() {
        match conn {
            Ok(mut stream) => {
                let mut buf = [0u8; 64];
                let n = stream.read(&mut buf).unwrap_or(0);
                let cmd = &buf[..n];

                match cmd {
                    c if c == TOGGLE_CMD => {
                        toggle_flag.store(true, Ordering::SeqCst);
                        let _ = stream.write_all(b"ok");
                    }
                    c if c == STATUS_CMD => {
                        let _ = stream.write_all(b"running");
                    }
                    _ => {}
                }
            }
            Err(e) => {
                tracing::warn!("IPC connection error: {}", e);
            }
        }
    }
    Ok(())
}
