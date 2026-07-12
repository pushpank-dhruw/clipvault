// Live palette from the current Omarchy theme (~/.config/omarchy/current/theme/
// colors.toml), re-read whenever the theme changes. Falls back to Tokyo Night
// on non-Omarchy systems (file missing → defaults).

pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Io
import QtQuick

QtObject {
    id: theme

    // Parsed `key = "#hex"` map from colors.toml.
    property var raw: ({})

    function _c(key, fallback) {
        return theme.raw[key] !== undefined ? theme.raw[key] : fallback;
    }

    // Semantic palette used across the shelf.
    readonly property color bg: theme._c("background", "#1a1b26")
    readonly property color fg: theme._c("foreground", "#c0caf5")
    readonly property color accent: theme._c("accent", "#7aa2f7")
    readonly property color onAccent: theme._c("background", "#1a1b26")
    readonly property color surface: theme._c("color0", "#24283b")
    readonly property color surfaceHover: Qt.lighter(theme.surface, 1.6)
    readonly property color line: Qt.lighter(theme.surface, 2.2)
    readonly property color muted: theme._c("color8", "#565f89")
    readonly property color favorite: theme._c("color3", "#e0af68")
    readonly property color danger: theme._c("color1", "#f7768e")

    function _parse(txt) {
        const map = {};
        const re = /^\s*([A-Za-z0-9_]+)\s*=\s*"?(#?[0-9A-Fa-f]{6})"?/;
        const lines = (txt || "").split("\n");
        for (let i = 0; i < lines.length; i++) {
            const m = lines[i].match(re);
            if (m)
                map[m[1]] = m[2][0] === "#" ? m[2] : "#" + m[2];
        }
        theme.raw = map;
    }

    property FileView file: FileView {
        path: (Quickshell.env("HOME") || "") + "/.config/omarchy/current/theme/colors.toml"
        watchChanges: true
        onLoaded: theme._parse(text())
        onFileChanged: reload()
        onLoadFailed: theme.raw = ({})
    }
}
