use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use clipvault::config::Config;
use clipvault::gui::ClipboardApp;
use clipvault::ipc;
use clipvault::monitor::ClipboardMonitor;
use clipvault::store::Store;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "clipvault",
    version,
    about = "Clipboard history manager with Wayland-native monitoring"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Toggle the GUI overlay window
    Toggle,
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
            if !Config::socket_path().exists() {
                run_daemon(config)?;
            } else {
                let socket_path = Config::socket_path();
                match ipc::send_command(&socket_path, ipc::TOGGLE_CMD) {
                    Ok(resp) => println!("{}", resp.trim()),
                    Err(e) => {
                        eprintln!("Daemon not responding, starting new: {}", e);
                        run_daemon(config)?;
                    }
                }
            }
            Ok(())
        }
        Some(Commands::List { format: _ }) => {
            let db_path = Config::db_path()?;
            let store = Store::open(&db_path, config.max_entries)?;
            let entries = store.list(100, 0)?;
            println!("{}", serde_json::to_string_pretty(&entries)?);
            Ok(())
        }
        Some(Commands::Search { query }) => {
            let db_path = Config::db_path()?;
            let store = Store::open(&db_path, config.max_entries)?;
            let entries = store.search(&query, 100)?;
            println!("{}", serde_json::to_string_pretty(&entries)?);
            Ok(())
        }
        Some(Commands::Clear) => {
            let db_path = Config::db_path()?;
            let mut store = Store::open(&db_path, config.max_entries)?;
            store.clear()?;
            println!("Clipboard history cleared");
            Ok(())
        }
        Some(Commands::Status) => {
            let socket_path = Config::socket_path();
            let db_path = Config::db_path()?;
            let store = Store::open(&db_path, config.max_entries)?;
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
        None => {
            run_daemon(config)?;
            Ok(())
        }
    }
}

fn run_daemon(config: Config) -> Result<()> {
    let db_path = Config::db_path()?;
    let store = Store::open(&db_path, config.max_entries)?;
    let store = Arc::new(Mutex::new(store));

    let toggle_flag = Arc::new(AtomicBool::new(false));
    let store_for_gui = store.clone();

    let ipc_toggle = toggle_flag.clone();
    let socket_path = Config::socket_path();

    std::thread::spawn(move || {
        if let Err(e) = ipc::listen(&socket_path, ipc_toggle) {
            tracing::error!("IPC listener failed: {}", e);
        }
    });

    let store_for_monitor = store.clone();
    let monitor = ClipboardMonitor::new(config.poll_interval_ms);

    let last_hash = monitor.last_hash();
    if let Ok(content) = get_clipboard_text_now() {
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        *last_hash.lock().unwrap() = Some(hash);
    }

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        if let Err(e) = rt.block_on(monitor.run(move |content, _hash| {
            let source = get_active_app();
            if let Err(e) =
                store_for_monitor
                    .lock()
                    .unwrap()
                    .insert(&content, "text", source.as_deref())
            {
                tracing::error!("failed to store clipboard entry: {}", e);
            }
        })) {
            tracing::error!("clipboard monitor failed: {}", e);
        }
    });

    tracing::info!("clipvault daemon started");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([config.overlay_width, config.overlay_height])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_window_level(egui::WindowLevel::AlwaysOnTop),
        ..Default::default()
    };

    eframe::run_native(
        "clipvault",
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(GuiWrapper {
                app: ClipboardApp::new(store_for_gui),
                toggle_flag,
                window_closed: false,
            }))
        }),
    )
    .context("eframe failed")
}

struct GuiWrapper {
    app: ClipboardApp,
    toggle_flag: Arc<AtomicBool>,
    window_closed: bool,
}

impl eframe::App for GuiWrapper {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.window_closed {
            return;
        }

        if self.toggle_flag.swap(false, Ordering::SeqCst) {
            self.window_closed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.app.update(ctx, _frame);
    }

    fn on_exit(&mut self) {
        let _ = std::fs::remove_file(Config::socket_path());
    }
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
