use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use clipvault::config::Config;
use clipvault::gui::ClipboardApp;
use clipvault::ipc::{self, IpcState};
use clipvault::monitor::{ClipboardContent, ClipboardMonitor};
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
    /// Toggle the clipboard shelf window
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
            launch_or_signal(config, true);
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
            let db_path = Config::db_path()?;
            let store = Store::open(
                &db_path,
                config.max_entries,
                config.max_image_entries,
                Config::images_dir().ok(),
            )?;
            let entries = store.list(100, 0)?;
            println!("{}", serde_json::to_string_pretty(&entries)?);
            Ok(())
        }
        Some(Commands::Search { query }) => {
            let db_path = Config::db_path()?;
            let store = Store::open(
                &db_path,
                config.max_entries,
                config.max_image_entries,
                Config::images_dir().ok(),
            )?;
            let entries = store.search(&query, 100)?;
            println!("{}", serde_json::to_string_pretty(&entries)?);
            Ok(())
        }
        Some(Commands::Clear) => {
            let db_path = Config::db_path()?;
            let mut store = Store::open(
                &db_path,
                config.max_entries,
                config.max_image_entries,
                Config::images_dir().ok(),
            )?;
            store.clear()?;
            println!("Clipboard history cleared");
            Ok(())
        }
        Some(Commands::Status) => {
            let socket_path = Config::socket_path();
            let db_path = Config::db_path()?;
            let store = Store::open(
                &db_path,
                config.max_entries,
                config.max_image_entries,
                Config::images_dir().ok(),
            )?;
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
            run_daemon(config, false)?;
            Ok(())
        }
    }
}

fn launch_or_signal(config: Config, _start_visible: bool) {
    let socket_path = Config::socket_path();

    if socket_path.exists() {
        match ipc::send_command(&socket_path, ipc::TOGGLE_CMD) {
            Ok(_) => return,
            Err(_) => {
                let _ = std::fs::remove_file(&socket_path);
            }
        }
    }

    if let Err(e) = run_daemon(config, true) {
        eprintln!("Failed to start daemon: {}", e);
    }
}

fn run_daemon(config: Config, start_visible: bool) -> Result<()> {
    let db_path = Config::db_path()?;
    let images_dir = Config::images_dir()?;
    let store = Store::open(
        &db_path,
        config.max_entries,
        config.max_image_entries,
        Some(images_dir),
    )?;
    let store = Arc::new(Mutex::new(store));

    let toggle_flag = Arc::new(AtomicBool::new(false));
    let quit_flag = Arc::new(AtomicBool::new(false));
    let store_for_gui = store.clone();

    let ipc_state = IpcState {
        toggle_flag: toggle_flag.clone(),
        quit_flag: quit_flag.clone(),
        store: store.clone(),
    };
    let socket_path = Config::socket_path();

    std::thread::spawn(move || {
        if let Err(e) = ipc::listen(&socket_path, ipc_state) {
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
            match content {
                ClipboardContent::Text(text) => {
                    if let Err(e) =
                        store_for_monitor
                            .lock()
                            .unwrap()
                            .insert(&text, "text", source.as_deref())
                    {
                        tracing::error!("failed to store clipboard entry: {}", e);
                    }
                }
                ClipboardContent::Image { data, mime_type } => {
                    if let Err(e) = store_for_monitor.lock().unwrap().insert_image(
                        &data,
                        &mime_type,
                        source.as_deref(),
                    ) {
                        tracing::error!("failed to store image: {}", e);
                    }
                }
            }
        })) {
            tracing::error!("clipboard monitor failed: {}", e);
        }
    });

    tracing::info!("clipvault daemon started");

    // Invisible 1x1 host window. On Wayland a mapped toplevel cannot be hidden
    // again (Visible(false) is a no-op), so real windows live in child
    // viewports below: dropping a show_viewport_immediate call destroys its
    // surface, which is the only reliable hide.
    let viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size([2.0, 2.0])
        .with_visible(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_app_id("clipvault");

    let native_options = eframe::NativeOptions {
        viewport: viewport_builder,
        ..Default::default()
    };

    let shelf_size = [config.shelf_width, config.shelf_height];
    eframe::run_native(
        "clipvault",
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(GuiWrapper {
                app: ClipboardApp::new(store_for_gui, &config),
                toggle_flag,
                quit_flag,
                shelf_visible: start_visible,
                shelf_size,
            }))
        }),
    )
    .context("eframe failed")
}

struct GuiWrapper {
    app: ClipboardApp,
    toggle_flag: Arc<AtomicBool>,
    quit_flag: Arc<AtomicBool>,
    shelf_visible: bool,
    shelf_size: [f32; 2],
}

impl eframe::App for GuiWrapper {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.quit_flag.swap(false, Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if self.toggle_flag.swap(false, Ordering::SeqCst) {
            self.shelf_visible = !self.shelf_visible;
        }

        if self.shelf_visible {
            let builder = egui::ViewportBuilder::default()
                .with_title("clipvault")
                .with_app_id("clipvault-shelf")
                .with_inner_size(self.shelf_size)
                .with_decorations(false)
                .with_transparent(true)
                .with_window_level(egui::WindowLevel::AlwaysOnTop);

            let mut hide = false;
            let app = &mut self.app;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("clipvault-shelf"),
                builder,
                |ctx, _class| {
                    let modal_was_open = app.modal_open();
                    app.ui(ctx);

                    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                    if app.should_hide
                        || (escape && !modal_was_open && !ctx.wants_keyboard_input())
                        || ctx.input(|i| i.viewport().close_requested())
                    {
                        hide = true;
                    }
                },
            );
            if hide {
                self.shelf_visible = false;
            }
        }

        // Keep the hidden host ticking so IPC flags are polled while idle.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
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
