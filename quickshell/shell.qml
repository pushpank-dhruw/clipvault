// clipvault — Quickshell frontend (root).
//
// Owns the connection to the headless `clipvault` daemon (unix socket, JSON
// line protocol), the entry model, and command helpers. Hosts the Shelf window
// and an IpcHandler so `qs -c clipvault ipc call shelf toggle` shows/hides it.
//
// Run:  qs -c clipvault      (after installing this dir to ~/.config/quickshell/clipvault)
// The daemon must be running:  clipvault   (or the systemd user service)

pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Io
import QtQuick

ShellRoot {
    id: root

    // ----- UI state (shared with the Shelf) -----
    property bool open: false
    property string filter: "all" // all | text | image | favorites
    property string query: ""
    property string category: "" // "" = all categories
    property var categories: [] // [{id,name,color}] from the daemon
    property var entries: [] // JS array of entry objects from the daemon
    property var counts: ({ total: 0, text: 0, image: 0, favorites: 0 })
    property string ocrText: ""

    // Hover-open bookkeeping: a shelf opened by the hot-zone auto-closes when
    // the pointer leaves it, unless the search box is focused (keepOpen). A
    // shelf opened by the keybind stays until dismissed.
    property bool hoverOpened: false
    property bool keepOpen: false

    // Live config mirror of ~/.config/clipvault/config.toml, fetched from the
    // daemon as JSON. The settings window edits it via set_config.
    property var config: ({})
    property bool settingsOpen: false

    // Live color palette from the current system (Omarchy) theme.
    readonly property Theme theme: Theme {}

    readonly property string socketPath:
        (Quickshell.env("XDG_RUNTIME_DIR") || "/run/user/1000") + "/clipvault.sock"

    // ----- request/response plumbing -----
    property int _reqId: 0
    property var _pending: ({})

    function _send(cmd, args, cb) {
        if (!reqSock.connected)
            return;
        const id = ++root._reqId;
        if (cb)
            root._pending[id] = cb;
        reqSock.write(JSON.stringify({ id: id, cmd: cmd, args: args || {} }) + "\n");
        reqSock.flush();
    }

    function refresh() {
        if (root.query.length > 0) {
            root._send("search", { query: root.query, limit: 200 }, r => {
                if (r.ok) root.entries = r.data;
            });
        } else {
            const args = { limit: root.config.shelf_max_entries || 60 };
            if (root.filter !== "all")
                args.filter = root.filter;
            if (root.category !== "")
                args.category = root.category;
            root._send("list", args, r => {
                if (r.ok) root.entries = r.data;
            });
        }
        root._send("counts", {}, r => {
            if (r.ok) root.counts = r.data;
        });
    }

    function setFilter(f) {
        root.filter = f;
        root.query = "";
        root.refresh();
    }
    function setCategoryFilter(cat) {
        root.category = cat || "";
        root.refresh();
    }
    function loadCategories() {
        root._send("categories", {}, r => {
            if (r.ok) root.categories = r.data;
        });
    }
    function setQuery(q) {
        root.query = q;
        root.refresh();
    }
    function paste(id) {
        root._send("paste", { id: id });
        root.open = false;
    }
    function toggleFavorite(id) { root._send("favorite", { id: id }, () => root.refresh()); }
    function del(id) { root._send("delete", { id: id }, () => root.refresh()); }
    function setCategory(id, cat) { root._send("set_category", { id: id, category: cat }, () => root.refresh()); }
    function runOcr(id) {
        root._send("ocr", { id: id }, r => {
            root.ocrText = r.ok ? (r.data.text || "(no text found)") : ("OCR error: " + r.error);
        });
    }

    function openHover() {
        root.open = true;
        root.hoverOpened = true;
    }
    function cancelClose() { closeTimer.stop(); }
    function scheduleClose() {
        if (root.hoverOpened && !root.keepOpen)
            closeTimer.restart();
    }

    function loadConfig() {
        root._send("config", {}, r => {
            if (r.ok) root.config = r.data;
        });
    }
    function saveConfig(patch) { root._send("set_config", patch); }

    function _handleLine(line) {
        if (!line)
            return;
        let msg;
        try {
            msg = JSON.parse(line);
        } catch (e) {
            return;
        }
        if (msg.event !== undefined) {
            if (msg.event === "changed") {
                root.refresh();
            } else if (msg.event === "toggle") {
                root.open = !root.open;
                root.hoverOpened = false;
            } else if (msg.event === "config") {
                root.loadConfig();
            }
            return;
        }
        if (msg.id !== undefined && root._pending[msg.id]) {
            const cb = root._pending[msg.id];
            delete root._pending[msg.id];
            cb(msg);
        }
    }

    // ----- daemon sockets -----
    // Two connections: one for request/response, one for the event stream. A
    // subscribed connection on the daemon becomes event-only (it stops reading
    // requests), so commands need their own socket.
    Socket {
        id: reqSock
        path: root.socketPath
        connected: true
        parser: SplitParser {
            onRead: line => root._handleLine(line)
        }
        onConnectedChanged: {
            if (connected) {
                root.refresh();
                root.loadConfig();
                root.loadCategories();
            }
        }
        onError: err => console.warn("clipvault req socket error:", err)
    }

    Socket {
        id: evtSock
        path: root.socketPath
        connected: true
        parser: SplitParser {
            onRead: line => root._handleLine(line)
        }
        onConnectedChanged: {
            if (connected) {
                write(JSON.stringify({ cmd: "subscribe" }) + "\n");
                flush();
            }
        }
        onError: err => console.warn("clipvault event socket error:", err)
    }

    // reconnect either socket if the daemon restarts
    Timer {
        interval: 2000
        running: true
        repeat: true
        onTriggered: {
            if (!reqSock.connected) reqSock.connected = true;
            if (!evtSock.connected) evtSock.connected = true;
        }
    }

    // grace period before a hover-opened shelf closes on pointer-leave
    Timer {
        id: closeTimer
        interval: root.config.notch_hover_close_delay_ms || 450
        onTriggered: {
            if (!root.keepOpen) {
                root.open = false;
                root.hoverOpened = false;
            }
        }
    }

    // re-sync whenever the shelf opens, in case a push was missed while hidden
    onOpenChanged: if (open) root.refresh()

    HotZone { shell: root }
    Shelf { shell: root }
    Settings { shell: root }

    IpcHandler {
        target: "shelf"
        function toggle(): void { root.open = !root.open; root.hoverOpened = false; }
        function reveal(): void { root.open = true; root.hoverOpened = false; }
        function hide(): void { root.open = false; root.hoverOpened = false; }
        function settings(): void { root.settingsOpen = true; }
    }
}
