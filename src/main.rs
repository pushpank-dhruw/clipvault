use anyhow::Result;
use clap::{Parser, Subcommand};
use clipvault::config::Config;
use clipvault::ipc::{self, IpcState};
use clipvault::monitor::{ClipboardContent, ClipboardMonitor};
use clipvault::store::Store;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "clipvault",
    version,
    about = "Headless clipboard-history daemon with Wayland-native monitoring"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Toggle the clipboard shelf (signals the running Quickshell frontend)
    Toggle,
    /// Quit the running daemon
    Quit,
    /// List clipboard history as JSON
    List {
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// Search clipboard history
    Search { query: String },
    /// Clear all clipboard history
    Clear,
    /// Show daemon status
    Status,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = Config::load()?;

    match cli.command {
        Some(Commands::Toggle) => {
            toggle_or_start(config);
            Ok(())
        }
        Some(Commands::Quit) => {
            let socket_path = Config::socket_path();
            if socket_path.exists() {
                match ipc::send_command(&socket_path, ipc::QUIT_CMD) {
                    Ok(_) => println!("Daemon stopped"),
                    Err(e) => eprintln!("Failed to stop daemon: {}", e),
                }
            } else {
                println!("Daemon not running");
            }
            Ok(())
        }
        Some(Commands::List { format: _ }) => {
            let store = open_store(&config)?;
            let entries = store.list(100, 0)?;
            println!("{}", serde_json::to_string_pretty(&entries)?);
            Ok(())
        }
        Some(Commands::Search { query }) => {
            let store = open_store(&config)?;
            let entries = store.search(&query, 100)?;
            println!("{}", serde_json::to_string_pretty(&entries)?);
            Ok(())
        }
        Some(Commands::Clear) => {
            let mut store = open_store(&config)?;
            store.clear()?;
            println!("Clipboard history cleared");
            Ok(())
        }
        Some(Commands::Status) => {
            let socket_path = Config::socket_path();
            let store = open_store(&config)?;
            let count = store.count()?;
            if socket_path.exists() {
                println!("Daemon: running");
            } else {
                println!("Daemon: not running");
            }
            println!("Entries: {}", count);
            println!("Socket: {}", socket_path.display());
            Ok(())
        }
        None => run_daemon(config),
    }
}

fn open_store(config: &Config) -> Result<Store> {
    let db_path = Config::db_path()?;
    Store::open(
        &db_path,
        config.max_entries,
        config.max_image_entries,
        Config::images_dir().ok(),
    )
}

/// `clipvault toggle`: signal the running daemon (which pushes a `toggle`
/// event to the subscribed frontend), or start the daemon if none is running.
fn toggle_or_start(config: Config) {
    let socket_path = Config::socket_path();

    if socket_path.exists() {
        match ipc::send_command(&socket_path, ipc::TOGGLE_CMD) {
            Ok(_) => return,
            Err(_) => {
                let _ = std::fs::remove_file(&socket_path);
            }
        }
    }

    if let Err(e) = run_daemon(config) {
        eprintln!("Failed to start daemon: {}", e);
    }
}

/// Run the headless daemon: clipboard monitor + IPC listener. Blocks until a
/// `quit` command is received. The GUI now lives in a separate Quickshell
/// process that talks to this daemon over the socket.
fn run_daemon(config: Config) -> Result<()> {
    let db_path = Config::db_path()?;
    let images_dir = Config::images_dir()?;
    let store = Store::open(
        &db_path,
        config.max_entries,
        config.max_image_entries,
        Some(images_dir),
    )?;
    let store = Arc::new(Mutex::new(store));

    let quit_flag = Arc::new(AtomicBool::new(false));
    let subscribers: ipc::Subscribers = Arc::new(Mutex::new(Vec::new()));
    let shared_config = Arc::new(Mutex::new(config));

    let ipc_state = IpcState {
        quit_flag: quit_flag.clone(),
        store: store.clone(),
        subscribers: subscribers.clone(),
        config: shared_config.clone(),
    };
    let socket_path = Config::socket_path();

    {
        let socket_path = socket_path.clone();
        std::thread::spawn(move || {
            if let Err(e) = ipc::listen(&socket_path, ipc_state) {
                tracing::error!("IPC listener failed: {}", e);
            }
        });
    }

    let store_for_monitor = store.clone();
    let subs_for_monitor = subscribers.clone();
    let monitor = ClipboardMonitor::new(shared_config.clone());

    // Seed the last-seen hash with the current clipboard so the daemon doesn't
    // re-capture whatever was already copied before it started.
    let last_hash = monitor.last_hash();
    if let Ok(content) = get_clipboard_text_now() {
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        *last_hash.lock().unwrap() = Some(hash);
    }

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        if let Err(e) = rt.block_on(monitor.run(move |content, _hash| {
            let source = get_active_app();
            let stored = match content {
                ClipboardContent::Text(text) => store_for_monitor
                    .lock()
                    .unwrap()
                    .insert(&text, "text", source.as_deref())
                    .map(|_| ()),
                ClipboardContent::Image { data, mime_type } => store_for_monitor
                    .lock()
                    .unwrap()
                    .insert_image(&data, &mime_type, source.as_deref())
                    .map(|_| ()),
            };
            match stored {
                Ok(()) => ipc::broadcast(&subs_for_monitor, ipc::CHANGED_EVENT),
                Err(e) => tracing::error!("failed to store clipboard entry: {}", e),
            }
        })) {
            tracing::error!("clipboard monitor failed: {}", e);
        }
    });

    // Reload config when config.toml changes on disk (hand edits), applying it
    // live and notifying the frontend. The settings window writes the same file,
    // so equal reloads are skipped to avoid redundant churn.
    {
        let cfg = shared_config.clone();
        let store_w = store.clone();
        let subs_w = subscribers.clone();
        std::thread::spawn(move || {
            let Ok(path) = Config::path() else { return };
            let mut last = file_mtime(&path);
            loop {
                std::thread::sleep(Duration::from_secs(2));
                let cur = file_mtime(&path);
                if cur == last {
                    continue;
                }
                last = cur;
                let Ok(loaded) = Config::load() else { continue };
                if cfg.lock().map(|c| *c == loaded).unwrap_or(true) {
                    continue;
                }
                if let Ok(mut s) = store_w.lock() {
                    s.set_limits(loaded.max_entries, loaded.max_image_entries);
                }
                if let Ok(mut c) = cfg.lock() {
                    *c = loaded;
                }
                ipc::broadcast(&subs_w, ipc::CONFIG_EVENT);
                tracing::info!("config reloaded from disk");
            }
        });
    }

    tracing::info!("clipvault daemon started (headless)");

    while !quit_flag.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(200));
    }

    tracing::info!("clipvault daemon stopping");
    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

fn file_mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

fn get_clipboard_text_now() -> Result<String> {
    let output = std::process::Command::new("wl-paste").output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Ok(String::new())
    }
}

fn get_active_app() -> Option<String> {
    std::process::Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                serde_json::from_slice::<serde_json::Value>(&o.stdout)
                    .ok()
                    .and_then(|v| v["class"].as_str().map(|s| s.to_string()))
            } else {
                None
            }
        })
}
