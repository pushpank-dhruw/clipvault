// Invisible top-center hot-zone. A thin always-present layer-shell strip that
// opens the shelf when the pointer dwells on it — replacing the old Rust
// cursor-polling watcher (hover.rs) with a native HoverHandler.

import Quickshell
import Quickshell.Wayland
import QtQuick

PanelWindow {
    id: zone
    required property var shell

    // Disabled when `notch_hover` is turned off in settings.
    visible: zone.shell.config.notch_hover !== false

    anchors.top: true
    exclusiveZone: 0
    implicitWidth: zone.shell.config.notch_hover_width || 320
    implicitHeight: 6
    color: "transparent"

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    WlrLayershell.namespace: "clipvault-hotzone"

    HoverHandler {
        id: hh
        onHoveredChanged: {
            if (hovered) {
                zone.shell.cancelClose();
                dwell.restart();
            } else {
                dwell.stop();
                zone.shell.scheduleClose();
            }
        }
    }

    // Require a short dwell before opening, to avoid opening on pass-through.
    Timer {
        id: dwell
        interval: zone.shell.config.notch_hover_dwell_ms || 120
        onTriggered: zone.shell.openHover()
    }
}
