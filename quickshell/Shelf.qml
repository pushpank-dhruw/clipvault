// The notch shelf: a top-center wlr-layer-shell surface (replaces the old
// egui floating toplevel + hyprctl repositioning). Anchored to the top with
// no exclusive zone, so it floats over content and centers horizontally.

import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

PanelWindow {
    id: shelf
    required property var shell

    // Palette follows the current system theme (see Theme.qml).
    readonly property color bg: shell.theme.bg
    readonly property color surface: shell.theme.surface
    readonly property color surfaceHover: shell.theme.surfaceHover
    readonly property color line: shell.theme.line
    readonly property color fg: shell.theme.fg
    readonly property color muted: shell.theme.muted
    readonly property color accent: shell.theme.accent

    // Smooth, subtle drop-in / fade-out (Supaste-style notch). `anim` eases
    // 0↔1 with the open state; the surface stays mapped until the fade-out
    // finishes so the exit animation plays.
    property real anim: shell.open ? 1 : 0
    Behavior on anim { NumberAnimation { duration: 220; easing.type: Easing.OutQuint } }
    visible: anim > 0.001

    // Top-center notch: only the top anchor is set, so layer-shell centers us
    // horizontally. No exclusive zone → we overlay rather than reserve space.
    anchors.top: true
    margins.top: 8
    exclusiveZone: 0
    implicitWidth: shell.config.shelf_width || 860
    implicitHeight: shell.config.shelf_height || 250
    color: "transparent"

    WlrLayershell.layer: WlrLayer.Overlay
    // Grab the keyboard for a toggle-opened shelf so arrow/Enter navigation and
    // search work; a hover-opened shelf stays hands-off (no focus steal).
    WlrLayershell.keyboardFocus: (shell.open && !shell.hoverOpened)
        ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.OnDemand
    WlrLayershell.namespace: "clipvault-shelf"

    // Keep a hover-opened shelf alive while the pointer is over it, and close
    // it (after the grace period) once the pointer leaves.
    HoverHandler {
        onHoveredChanged: hovered ? shelf.shell.cancelClose() : shelf.shell.scheduleClose()
    }

    // A focused search box pins the shelf open.
    Binding {
        target: shelf.shell
        property: "keepOpen"
        value: search.activeFocus
    }

    // Focus the card list when the shelf opens, so arrow keys work right away.
    Connections {
        target: shelf.shell
        function onOpenChanged() {
            if (shelf.shell.open) {
                list.currentIndex = 0;
                list.forceActiveFocus();
            }
        }
    }

    Rectangle {
        id: panel
        anchors.fill: parent
        radius: 14
        color: shelf.bg
        border.color: shelf.line
        border.width: 1
        // Subtle drop + grow from the top edge (the notch), fading in.
        opacity: shelf.anim
        transform: [
            Translate { y: (1.0 - shelf.anim) * -16 },
            Scale {
                origin.x: panel.width / 2
                origin.y: 0
                xScale: 0.98 + 0.02 * shelf.anim
                yScale: 0.98 + 0.02 * shelf.anim
            }
        ]

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 10

            // ---- header: tabs + search ----
            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                Repeater {
                    model: [
                        { key: "all", label: "All" },
                        { key: "text", label: "Text" },
                        { key: "image", label: "Images" },
                        { key: "favorites", label: "★ Favorites" }
                    ]
                    delegate: Rectangle {
                        id: tab
                        required property var modelData
                        readonly property bool active: shelf.shell.filter === modelData.key
                        radius: 8
                        implicitHeight: 30
                        implicitWidth: tabLabel.implicitWidth + 22
                        color: active ? shelf.accent : shelf.surface
                        Text {
                            id: tabLabel
                            anchors.centerIn: parent
                            text: tab.modelData.label
                            color: tab.active ? shelf.bg : shelf.fg
                            font.pixelSize: 12
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: shelf.shell.setFilter(tab.modelData.key)
                        }
                    }
                }

                ComboBox {
                    id: catCombo
                    Layout.preferredWidth: 130
                    Layout.preferredHeight: 30
                    model: {
                        const m = [{ name: "", label: "All tags" }];
                        const cats = shelf.shell.categories || [];
                        for (let i = 0; i < cats.length; i++)
                            m.push({ name: cats[i].name, label: cats[i].name });
                        return m;
                    }
                    textRole: "label"
                    valueRole: "name"
                    onActivated: shelf.shell.setCategoryFilter(currentValue)
                    background: Rectangle {
                        radius: 8
                        color: shelf.surface
                        border.color: shelf.line
                    }
                    contentItem: Text {
                        leftPadding: 10
                        text: catCombo.displayText
                        color: shelf.fg
                        font.pixelSize: 12
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                    indicator: Text {
                        x: catCombo.width - width - 10
                        y: (catCombo.height - height) / 2
                        text: "▾"
                        color: shelf.muted
                        font.pixelSize: 12
                    }
                    delegate: ItemDelegate {
                        id: catItem
                        required property var modelData
                        required property int index
                        width: catCombo.width
                        height: 28
                        contentItem: Text {
                            leftPadding: 8
                            text: catItem.modelData.label
                            color: shelf.fg
                            font.pixelSize: 12
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                        background: Rectangle {
                            color: catCombo.highlightedIndex === catItem.index ? shelf.surfaceHover : "transparent"
                        }
                    }
                    popup: Popup {
                        y: catCombo.height + 4
                        width: catCombo.width
                        padding: 4
                        background: Rectangle {
                            radius: 8
                            color: shelf.surface
                            border.color: shelf.line
                        }
                        contentItem: ListView {
                            clip: true
                            implicitHeight: Math.min(contentHeight, 320)
                            model: catCombo.popup.visible ? catCombo.delegateModel : null
                            currentIndex: catCombo.highlightedIndex
                        }
                    }
                }

                Item { Layout.fillWidth: true }

                TextField {
                    id: search
                    Layout.preferredWidth: 240
                    Layout.preferredHeight: 30
                    placeholderText: "Search…"
                    color: shelf.fg
                    placeholderTextColor: shelf.muted
                    leftPadding: 10
                    verticalAlignment: TextInput.AlignVCenter
                    background: Rectangle { radius: 8; color: shelf.surface; border.color: shelf.line }
                    text: shelf.shell.query
                    onTextEdited: shelf.shell.setQuery(text)
                    Keys.onEscapePressed: shelf.shell.open = false
                }

                Rectangle {
                    implicitWidth: 30
                    implicitHeight: 30
                    radius: 8
                    color: shelf.surface
                    Text {
                        anchors.centerIn: parent
                        text: "⚙"
                        color: shelf.fg
                        font.pixelSize: 15
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: shelf.shell.settingsOpen = true
                    }
                }
            }

            // ---- card row ----
            ListView {
                id: list
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: ListView.Horizontal
                spacing: 10
                clip: true
                focus: true
                keyNavigationEnabled: true
                highlightMoveDuration: 140
                model: shelf.shell.entries

                function pasteCurrent() {
                    const e = shelf.shell.entries[list.currentIndex];
                    if (e)
                        shelf.shell.paste(e.id);
                }

                Keys.onReturnPressed: list.pasteCurrent()
                Keys.onEnterPressed: list.pasteCurrent()
                Keys.onDeletePressed: {
                    const e = shelf.shell.entries[list.currentIndex];
                    if (e)
                        shelf.shell.del(e.id);
                }
                // Type-to-search: a printable key jumps into the search box.
                Keys.onPressed: event => {
                    const c = event.text.length === 1 ? event.text.charCodeAt(0) : 0;
                    if (c >= 0x20 && c < 0x7f
                            && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
                        shelf.shell.setQuery(shelf.shell.query + event.text);
                        search.forceActiveFocus();
                        event.accepted = true;
                    }
                }

                delegate: Card {
                    required property var modelData
                    entry: modelData
                    shell: shelf.shell
                    height: list.height
                    selected: ListView.isCurrentItem
                }

                Text {
                    anchors.centerIn: parent
                    visible: list.count === 0
                    text: shelf.shell.query.length > 0 ? "No matches" : "Clipboard is empty"
                    color: shelf.muted
                }
            }

            // ---- OCR result strip ----
            Text {
                Layout.fillWidth: true
                visible: shelf.shell.ocrText.length > 0
                text: "OCR: " + shelf.shell.ocrText
                color: shelf.muted
                font.pixelSize: 11
                elide: Text.ElideRight
                MouseArea { anchors.fill: parent; onClicked: shelf.shell.ocrText = "" }
            }
        }
    }

    // Escape anywhere closes the shelf.
    Item {
        anchors.fill: parent
        focus: shelf.visible
        Keys.onEscapePressed: shelf.shell.open = false
    }
}
