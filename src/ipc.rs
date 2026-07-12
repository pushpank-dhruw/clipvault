//! Daemon IPC over a Unix socket.
//!
//! Two protocols share the socket:
//!
//! * **Legacy one-shot** (`toggle` / `quit` / `status`): a raw token, no
//!   newline, one response, connection closes. Used by the CLI (`clipvault
//!   toggle` keybind, `clipvault quit`). `toggle` is relayed to the frontend
//!   as a `toggle` event.
//! * **JSON line protocol**: newline-delimited `{"id":N,"cmd":"…","args":{…}}`
//!   requests, each answered by a `{"id":N,"ok":true,"data":…}` line. A client
//!   may send `{"cmd":"subscribe"}` to turn its connection into an event stream
//!   that receives unsolicited `{"event":"changed"}` lines when history changes.
//!   This is what the Quickshell (`Quickshell.Io.Socket`) frontend speaks.

use crate::config::Config;
use crate::store::Store;
use anyhow::{Context, Result};
use interprocess::local_socket::traits::ListenerExt;
use interprocess::local_socket::{ConnectOptions, GenericFilePath, ListenerOptions, ToFsName};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub const TOGGLE_CMD: &[u8] = b"toggle";
pub const QUIT_CMD: &[u8] = b"quit";
pub const STATUS_CMD: &[u8] = b"status";

/// Line pushed to every subscriber whenever the history changes.
pub const CHANGED_EVENT: &str = "{\"event\":\"changed\"}";
/// Line pushed to every subscriber to toggle the shelf's visibility.
pub const TOGGLE_EVENT: &str = "{\"event\":\"toggle\"}";
/// Line pushed to every subscriber when the config changes (settings window).
pub const CONFIG_EVENT: &str = "{\"event\":\"config\"}";

/// Registered event-stream senders. Each `subscribe` connection owns one
/// receiver; [`broadcast`] fans a message out to all and prunes dead ones.
pub type Subscribers = Arc<Mutex<Vec<Sender<String>>>>;

/// Send `msg` to every subscriber, dropping any whose receiver has hung up.
pub fn broadcast(subscribers: &Subscribers, msg: &str) {
    if let Ok(mut subs) = subscribers.lock() {
        subs.retain(|tx| tx.send(msg.to_string()).is_ok());
    }
}

/// Send a legacy one-shot command and read the raw response (used by the CLI).
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

pub struct IpcState {
    pub quit_flag: Arc<AtomicBool>,
    pub store: Arc<Mutex<Store>>,
    pub subscribers: Subscribers,
    pub config: Arc<Mutex<Config>>,
}

pub fn listen(socket_path: &Path, state: IpcState) -> Result<()> {
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

    let state = Arc::new(state);
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let state = state.clone();
                std::thread::spawn(move || handle_conn(stream, state));
            }
            Err(e) => {
                tracing::warn!("IPC connection error: {}", e);
            }
        }
    }
    Ok(())
}

/// Serve one connection: dispatch a legacy token, or read newline-delimited
/// JSON requests until EOF (or until the client subscribes, after which this
/// thread owns the stream for the lifetime of the event stream).
fn handle_conn<S: Read + Write>(mut stream: S, state: Arc<IpcState>) {
    let mut acc: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    let mut first = true;

    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        acc.extend_from_slice(&buf[..n]);

        // Legacy commands are a bare token with no newline; detect on the very
        // first chunk so they don't stall waiting for a delimiter.
        if first {
            first = false;
            match trim_ascii(&acc) {
                TOGGLE_CMD => {
                    broadcast(&state.subscribers, TOGGLE_EVENT);
                    let _ = stream.write_all(b"ok");
                    return;
                }
                QUIT_CMD => {
                    state.quit_flag.store(true, Ordering::SeqCst);
                    let _ = stream.write_all(b"bye");
                    return;
                }
                STATUS_CMD => {
                    let count = state
                        .store
                        .lock()
                        .map(|s| s.count().unwrap_or(0))
                        .unwrap_or(0);
                    let _ = stream.write_all(format!("{count}\n").as_bytes());
                    return;
                }
                _ => {}
            }
        }

        while let Some(pos) = acc.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = acc.drain(..=pos).collect();
            let line = &line[..line.len() - 1];
            if line.iter().all(|b| b.is_ascii_whitespace()) {
                continue;
            }
            if dispatch_line(line, &mut stream, &state) {
                return; // became an event stream; the thread now owns the socket
            }
        }
    }
}

/// Handle one JSON request line. Returns `true` when the connection has been
/// converted into a subscription (the caller must stop reading).
fn dispatch_line<S: Write>(line: &[u8], stream: &mut S, state: &Arc<IpcState>) -> bool {
    let req: Value = match serde_json::from_slice(line) {
        Ok(v) => v,
        Err(e) => {
            write_line(
                stream,
                &json!({ "ok": false, "error": format!("invalid json: {e}") }).to_string(),
            );
            return false;
        }
    };
    let id = req.get("id").cloned();
    let cmd = req.get("cmd").and_then(Value::as_str).unwrap_or("");

    if cmd == "subscribe" {
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        if let Ok(mut subs) = state.subscribers.lock() {
            subs.push(tx);
        }
        let mut ack = json!({ "ok": true, "data": "subscribed" });
        if let Some(id) = id {
            ack["id"] = id;
        }
        write_line(stream, &ack.to_string());
        for msg in rx {
            if stream.write_all(msg.as_bytes()).is_err()
                || stream.write_all(b"\n").is_err()
                || stream.flush().is_err()
            {
                break;
            }
        }
        return true;
    }

    let mut resp = handle_command(cmd, &req, state);
    if let Some(id) = id {
        resp["id"] = id;
    }
    write_line(stream, &resp.to_string());
    false
}

/// Execute a JSON command against the store and return a response object
/// (`{"ok":true,"data":…}` or `{"ok":false,"error":…}`), without the `id`.
fn handle_command(cmd: &str, req: &Value, state: &IpcState) -> Value {
    let args = req.get("args").cloned().unwrap_or(Value::Null);
    let arg = |k: &str| args.get(k);
    let limit = |dflt: usize| {
        arg("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(dflt)
    };
    let offset = arg("offset")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(0);

    match cmd {
        "list" => {
            let filter = arg("filter").and_then(Value::as_str);
            let category = arg("category").and_then(Value::as_str);
            let limit = limit(200);
            let store = match state.store.lock() {
                Ok(s) => s,
                Err(_) => return err_msg("store poisoned"),
            };
            let res = match (filter, category) {
                (Some("favorites"), _) => store.list_favorites(limit, offset),
                (Some(ft @ ("text" | "image")), Some(cat)) => {
                    store.list_by_type_and_category(ft, cat, limit, offset)
                }
                (Some(ft @ ("text" | "image")), None) => store.list_by_type(ft, limit, offset),
                (_, Some(cat)) => store.list_by_category(cat, limit, offset),
                _ => store.list(limit, offset),
            };
            ok_data(res)
        }
        "search" => {
            let query = arg("query").and_then(Value::as_str).unwrap_or("");
            let limit = limit(200);
            match state.store.lock() {
                Ok(store) => ok_data(store.search(query, limit)),
                Err(_) => err_msg("store poisoned"),
            }
        }
        "counts" => match state.store.lock() {
            Ok(store) => match store.type_counts() {
                Ok((total, text, image, favorites)) => json!({
                    "ok": true,
                    "data": { "total": total, "text": text, "image": image, "favorites": favorites }
                }),
                Err(e) => err(&e),
            },
            Err(_) => err_msg("store poisoned"),
        },
        "categories" => match state.store.lock() {
            Ok(store) => match store.list_categories() {
                Ok(cats) => {
                    let data: Vec<Value> = cats
                        .into_iter()
                        .map(|(id, name, color)| json!({ "id": id, "name": name, "color": color }))
                        .collect();
                    json!({ "ok": true, "data": data })
                }
                Err(e) => err(&e),
            },
            Err(_) => err_msg("store poisoned"),
        },
        "entry" => match arg("id").and_then(Value::as_i64) {
            Some(id) => match state.store.lock() {
                Ok(store) => ok_data(store.get_by_id(id)),
                Err(_) => err_msg("store poisoned"),
            },
            None => err_msg("missing id"),
        },
        "favorite" => match arg("id").and_then(Value::as_i64) {
            Some(id) => {
                let res = state.store.lock().map_err(|_| ()).and_then(|mut s| {
                    s.toggle_favorite(id).map_err(|_| ())
                });
                match res {
                    Ok(fav) => {
                        broadcast(&state.subscribers, CHANGED_EVENT);
                        json!({ "ok": true, "data": { "favorite": fav } })
                    }
                    Err(_) => err_msg("failed to toggle favorite"),
                }
            }
            None => err_msg("missing id"),
        },
        "set_category" => match arg("id").and_then(Value::as_i64) {
            Some(id) => {
                let category = arg("category").and_then(Value::as_str);
                let res = {
                    match state.store.lock() {
                        Ok(mut s) => s.set_category(id, category),
                        Err(_) => return err_msg("store poisoned"),
                    }
                };
                match res {
                    Ok(()) => {
                        broadcast(&state.subscribers, CHANGED_EVENT);
                        json!({ "ok": true })
                    }
                    Err(e) => err(&e),
                }
            }
            None => err_msg("missing id"),
        },
        "delete" => match arg("id").and_then(Value::as_i64) {
            Some(id) => {
                let res = {
                    match state.store.lock() {
                        Ok(mut s) => s.delete(id),
                        Err(_) => return err_msg("store poisoned"),
                    }
                };
                match res {
                    Ok(()) => {
                        broadcast(&state.subscribers, CHANGED_EVENT);
                        json!({ "ok": true })
                    }
                    Err(e) => err(&e),
                }
            }
            None => err_msg("missing id"),
        },
        "paste" => match arg("id").and_then(Value::as_i64) {
            Some(id) => paste_entry(state, id),
            None => err_msg("missing id"),
        },
        "ocr" => match arg("id").and_then(Value::as_i64) {
            Some(id) => {
                let data = match state.store.lock() {
                    Ok(store) => store.get_image_data(id),
                    Err(_) => return err_msg("store poisoned"),
                };
                match data {
                    Ok(Some(bytes)) => match crate::ocr::recognize(id, &bytes) {
                        Ok(text) => json!({ "ok": true, "data": { "text": text } }),
                        Err(e) => err(&e),
                    },
                    Ok(None) => err_msg("no image data for entry"),
                    Err(e) => err(&e),
                }
            }
            None => err_msg("missing id"),
        },
        "clear" => {
            let res = match state.store.lock() {
                Ok(mut s) => s.clear(),
                Err(_) => return err_msg("store poisoned"),
            };
            match res {
                Ok(()) => {
                    broadcast(&state.subscribers, CHANGED_EVENT);
                    json!({ "ok": true })
                }
                Err(e) => err(&e),
            }
        }
        "toggle" => {
            broadcast(&state.subscribers, TOGGLE_EVENT);
            json!({ "ok": true })
        }
        "quit" => {
            state.quit_flag.store(true, Ordering::SeqCst);
            json!({ "ok": true })
        }
        "status" => match state.store.lock() {
            Ok(store) => json!({ "ok": true, "data": { "count": store.count().unwrap_or(0) } }),
            Err(_) => err_msg("store poisoned"),
        },
        "config" => match state.config.lock() {
            Ok(cfg) => json!({ "ok": true, "data": serde_json::to_value(&*cfg).unwrap_or(Value::Null) }),
            Err(_) => err_msg("config poisoned"),
        },
        "set_config" => set_config(state, &args),
        "reload" => reload_config(state),
        other => err_msg(&format!("unknown command: {other}")),
    }
}

/// Apply a partial config patch (from the settings window): merge over the
/// current config, persist to `config.toml`, apply runtime-updatable settings,
/// and notify subscribers.
fn set_config(state: &IpcState, args: &Value) -> Value {
    let mut cfg = match state.config.lock() {
        Ok(c) => c,
        Err(_) => return err_msg("config poisoned"),
    };
    let new_cfg = match merge_config(&cfg, args) {
        Ok(c) => c,
        Err(e) => return err_msg(&e),
    };
    if let Err(e) = new_cfg.save() {
        return err_msg(&format!("failed to save config: {e}"));
    }
    if let Ok(mut store) = state.store.lock() {
        store.set_limits(new_cfg.max_entries, new_cfg.max_image_entries);
    }
    *cfg = new_cfg;
    drop(cfg);
    broadcast(&state.subscribers, CONFIG_EVENT);
    json!({ "ok": true })
}

/// Re-read `config.toml` from disk (e.g. after an external edit) and apply it.
fn reload_config(state: &IpcState) -> Value {
    let loaded = match Config::load() {
        Ok(c) => c,
        Err(e) => return err_msg(&format!("failed to load config: {e}")),
    };
    if let Ok(mut store) = state.store.lock() {
        store.set_limits(loaded.max_entries, loaded.max_image_entries);
    }
    match state.config.lock() {
        Ok(mut cfg) => *cfg = loaded,
        Err(_) => return err_msg("config poisoned"),
    }
    broadcast(&state.subscribers, CONFIG_EVENT);
    json!({ "ok": true })
}

/// Merge a JSON patch object over a config, returning the new config. Pure (no
/// disk I/O) so it is unit-testable without touching the user's config file.
fn merge_config(base: &Config, patch: &Value) -> std::result::Result<Config, String> {
    let obj = patch
        .as_object()
        .filter(|o| !o.is_empty())
        .ok_or_else(|| "set_config requires a non-empty args object".to_string())?;
    let mut merged =
        serde_json::to_value(base).map_err(|e| format!("failed to serialize config: {e}"))?;
    for (k, v) in obj {
        merged[k] = v.clone();
    }
    serde_json::from_value(merged).map_err(|e| format!("invalid config value: {e}"))
}

/// Copy an entry back to the Wayland clipboard by id.
fn paste_entry(state: &IpcState, id: i64) -> Value {
    let entry = match state.store.lock() {
        Ok(store) => store.get_by_id(id),
        Err(_) => return err_msg("store poisoned"),
    };
    let entry = match entry {
        Ok(Some(e)) => e,
        Ok(None) => return err_msg("entry not found"),
        Err(e) => return err(&e),
    };

    if entry.content_type == "image" {
        let data = match state.store.lock() {
            Ok(store) => store.get_image_data(id),
            Err(_) => return err_msg("store poisoned"),
        };
        match data {
            Ok(Some(bytes)) => {
                let mime = entry.mime_type.as_deref().unwrap_or("image/png");
                set_clipboard_image(&bytes, mime);
                json!({ "ok": true })
            }
            Ok(None) => err_msg("no image data for entry"),
            Err(e) => err(&e),
        }
    } else {
        set_clipboard(&entry.content);
        json!({ "ok": true })
    }
}

fn write_line<S: Write>(stream: &mut S, s: &str) {
    let _ = stream.write_all(s.as_bytes());
    let _ = stream.write_all(b"\n");
    let _ = stream.flush();
}

fn ok_data<T: serde::Serialize>(res: Result<T>) -> Value {
    match res {
        Ok(data) => json!({ "ok": true, "data": serde_json::to_value(data).unwrap_or(Value::Null) }),
        Err(e) => err(&e),
    }
}

fn err(e: &anyhow::Error) -> Value {
    json!({ "ok": false, "error": e.to_string() })
}

fn err_msg(msg: &str) -> Value {
    json!({ "ok": false, "error": msg })
}

fn trim_ascii(b: &[u8]) -> &[u8] {
    let start = b
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(b.len());
    let end = b
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map(|i| i + 1)
        .unwrap_or(start);
    &b[start..end]
}

fn set_clipboard(content: &str) {
    let Ok(mut child) = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    else {
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(content.as_bytes());
    }
    let _ = child.wait();
}

fn set_clipboard_image(data: &[u8], mime: &str) {
    let Ok(mut child) = std::process::Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(std::process::Stdio::piped())
        .spawn()
    else {
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(data);
    }
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn state_with_store() -> (IpcState, PathBuf) {
        use std::sync::atomic::AtomicU64;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("clipvault_ipc_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db_path = dir.join("test.db");
        let store = Store::open(&db_path, 100, 50, None).unwrap();
        let state = IpcState {
            quit_flag: Arc::new(AtomicBool::new(false)),
            store: Arc::new(Mutex::new(store)),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            config: Arc::new(Mutex::new(Config::default())),
        };
        (state, dir)
    }

    fn cmd(cmd: &str, args: Value, state: &IpcState) -> Value {
        handle_command(cmd, &json!({ "cmd": cmd, "args": args }), state)
    }

    #[test]
    fn lists_and_searches_entries() {
        let (state, _dir) = state_with_store();
        {
            let mut s = state.store.lock().unwrap();
            s.insert("cargo build --release", "text", None).unwrap();
            s.insert("https://example.com", "text", None).unwrap();
        }

        let listed = cmd("list", json!({}), &state);
        assert_eq!(listed["ok"], json!(true));
        assert_eq!(listed["data"].as_array().unwrap().len(), 2);

        let found = cmd("search", json!({ "query": "cargo" }), &state);
        assert_eq!(found["data"].as_array().unwrap().len(), 1);
        assert_eq!(found["data"][0]["content"], json!("cargo build --release"));
    }

    #[test]
    fn toggles_favorite_and_counts() {
        let (state, _dir) = state_with_store();
        let id = {
            let mut s = state.store.lock().unwrap();
            s.insert("note", "text", None).unwrap().id
        };

        let fav = cmd("favorite", json!({ "id": id }), &state);
        assert_eq!(fav["data"]["favorite"], json!(true));

        let counts = cmd("counts", json!({}), &state);
        assert_eq!(counts["data"]["favorites"], json!(1));
        assert_eq!(counts["data"]["total"], json!(1));
    }

    #[test]
    fn rejects_unknown_and_missing_args() {
        let (state, _dir) = state_with_store();
        assert_eq!(cmd("bogus", json!({}), &state)["ok"], json!(false));
        assert_eq!(cmd("favorite", json!({}), &state)["ok"], json!(false));
    }

    #[test]
    fn returns_and_merges_config() {
        let (state, _dir) = state_with_store();
        let got = handle_command("config", &json!({ "cmd": "config" }), &state);
        assert_eq!(got["ok"], json!(true));
        assert_eq!(got["data"]["max_entries"], json!(500));

        // merge_config is pure (no disk write) so it is safe to test directly.
        let base = Config::default();
        let patched = merge_config(&base, &json!({ "shelf_width": 999.0, "theme": "custom" })).unwrap();
        assert_eq!(patched.shelf_width, 999.0);
        assert_eq!(patched.theme, "custom");
        assert_eq!(patched.max_entries, base.max_entries);
        assert!(merge_config(&base, &json!({})).is_err());
        assert!(merge_config(&base, &json!({ "shelf_width": "not-a-number" })).is_err());
    }

    #[test]
    fn socket_roundtrip_and_push() {
        use interprocess::local_socket::{ConnectOptions, GenericFilePath, ToFsName};
        use std::io::{BufRead, BufReader};
        use std::time::Duration;

        let (state, _dir) = state_with_store();
        let subs = state.subscribers.clone();
        {
            let mut s = state.store.lock().unwrap();
            s.insert("hello socket", "text", None).unwrap();
        }

        let sock = std::env::temp_dir().join(format!(
            "clipvault_sock_{}_{:p}.sock",
            std::process::id(),
            &state as *const _
        ));
        let _ = std::fs::remove_file(&sock);
        let sock_listen = sock.clone();
        std::thread::spawn(move || {
            let _ = listen(&sock_listen, state);
        });
        for _ in 0..200 {
            if sock.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let connect = || {
            let name = sock.clone().to_fs_name::<GenericFilePath>().unwrap();
            ConnectOptions::new().name(name).connect_sync().unwrap()
        };

        // Request / response.
        let mut client = BufReader::new(connect());
        client
            .get_mut()
            .write_all(b"{\"id\":7,\"cmd\":\"list\"}\n")
            .unwrap();
        client.get_mut().flush().unwrap();
        let mut line = String::new();
        client.read_line(&mut line).unwrap();
        let resp: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(resp["id"], json!(7));
        assert_eq!(resp["ok"], json!(true));
        assert_eq!(resp["data"][0]["content"], json!("hello socket"));

        // Subscribe on a second connection, then verify a pushed event arrives.
        let mut sub = BufReader::new(connect());
        sub.get_mut().write_all(b"{\"cmd\":\"subscribe\"}\n").unwrap();
        sub.get_mut().flush().unwrap();
        let mut ack = String::new();
        sub.read_line(&mut ack).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&ack).unwrap()["ok"], json!(true));

        broadcast(&subs, CHANGED_EVENT);
        let mut event = String::new();
        sub.read_line(&mut event).unwrap();
        assert_eq!(event.trim(), CHANGED_EVENT);

        let _ = std::fs::remove_file(&sock);
    }

    #[test]
    fn broadcast_prunes_dead_subscribers() {
        let subs: Subscribers = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        subs.lock().unwrap().push(tx);
        broadcast(&subs, CHANGED_EVENT);
        assert_eq!(rx.recv().unwrap(), CHANGED_EVENT);
        assert_eq!(subs.lock().unwrap().len(), 1);

        drop(rx);
        broadcast(&subs, CHANGED_EVENT);
        assert_eq!(subs.lock().unwrap().len(), 0);
    }
}
