// Visual settings editor. Reads the daemon's config (JSON) and writes changes
// back via set_config; the daemon persists them to config.toml and pushes a
// `config` event so the shelf re-reads and applies live. Replaces hand-editing
// the TOML file.

import Quickshell
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

FloatingWindow {
    id: win
    required property var shell

    visible: shell.settingsOpen
    implicitWidth: 460
    implicitHeight: 580
    color: win.shell.theme.bg

    function num(key, dflt) {
        return win.shell.config[key] !== undefined ? win.shell.config[key] : dflt;
    }
    function flag(key, dflt) {
        return win.shell.config[key] !== undefined ? win.shell.config[key] : dflt;
    }

    // --- reusable rows (only user-driven signals write, to avoid feedback loops) ---
    component IntRow: RowLayout {
        id: row
        property string label
        property string key
        property int minv: 0
        property int maxv: 9999
        Layout.fillWidth: true
        Label { text: row.label; color: win.shell.theme.fg; Layout.fillWidth: true }
        SpinBox {
            from: row.minv
            to: row.maxv
            editable: true
            value: win.num(row.key, row.minv)
            onValueModified: win.shell.saveConfig({ [row.key]: value })
        }
    }

    component BoolRow: RowLayout {
        id: row
        property string label
        property string key
        Layout.fillWidth: true
        Label { text: row.label; color: win.shell.theme.fg; Layout.fillWidth: true }
        Switch {
            checked: win.flag(row.key, false)
            onToggled: win.shell.saveConfig({ [row.key]: checked })
        }
    }

    ScrollView {
        anchors.fill: parent
        anchors.margins: 16
        contentWidth: availableWidth

        ColumnLayout {
            width: win.width - 32
            spacing: 8

            Label { text: "Shelf"; color: win.shell.theme.accent; font.bold: true }
            IntRow { label: "Width"; key: "shelf_width"; minv: 400; maxv: 2400 }
            IntRow { label: "Height"; key: "shelf_height"; minv: 120; maxv: 700 }
            IntRow { label: "Thumbnail size"; key: "shelf_thumb_size"; minv: 32; maxv: 160 }
            IntRow { label: "Max cards"; key: "shelf_max_entries"; minv: 10; maxv: 200 }

            MenuSeparator { Layout.fillWidth: true }

            Label { text: "History"; color: win.shell.theme.accent; font.bold: true }
            IntRow { label: "Max text entries"; key: "max_entries"; minv: 50; maxv: 5000 }
            IntRow { label: "Max image entries"; key: "max_image_entries"; minv: 10; maxv: 500 }
            IntRow { label: "Poll interval (ms)"; key: "poll_interval_ms"; minv: 100; maxv: 3000 }

            MenuSeparator { Layout.fillWidth: true }

            Label { text: "Behaviour"; color: win.shell.theme.accent; font.bold: true }
            BoolRow { label: "Hover to open"; key: "notch_hover" }
            IntRow { label: "Hover dwell (ms)"; key: "notch_hover_dwell_ms"; minv: 0; maxv: 1000 }
            BoolRow { label: "Enable OCR"; key: "ocr_enabled" }
            BoolRow { label: "Hide sensitive (skip password-manager clips)"; key: "hide_sensitive" }

            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                Button {
                    text: "Close"
                    onClicked: win.shell.settingsOpen = false
                }
            }
        }
    }
}
