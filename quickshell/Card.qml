// One clipboard entry. Body click pastes (and closes the shelf); the top-right
// overlay holds favorite / OCR / delete, declared last so they sit above the
// full-card paste MouseArea.

import QtQuick

Item {
    id: card
    required property var entry
    required property var shell

    width: 190

    readonly property bool isImage: entry.content_type === "image"
    readonly property color fg: card.shell.theme.fg
    readonly property color muted: card.shell.theme.muted

    Rectangle {
        id: body
        anchors.fill: parent
        radius: 10
        color: hover.hovered ? card.shell.theme.surfaceHover : card.shell.theme.surface
        border.color: card.shell.theme.line
        border.width: 1

        // preview + meta
        Column {
            anchors.fill: parent
            anchors.margins: 10
            spacing: 6

            Item {
                width: parent.width
                height: parent.height - 26
                clip: true

                Image {
                    anchors.fill: parent
                    visible: card.isImage
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    cache: true
                    // Prefer the on-disk thumbnail; fall back to the full image.
                    source: card.isImage
                        ? "file://" + (card.entry.thumb_path || card.entry.content_path || "")
                        : ""
                }

                Text {
                    anchors.fill: parent
                    visible: !card.isImage
                    text: card.entry.content
                    color: card.fg
                    wrapMode: Text.WrapAnywhere
                    elide: Text.ElideRight
                    maximumLineCount: 7
                    font.family: "monospace"
                    font.pixelSize: 12
                }
            }

            Text {
                width: parent.width
                text: card.entry.source || (card.isImage ? "image" : "text")
                color: card.muted
                font.pixelSize: 10
                elide: Text.ElideRight
            }
        }

        HoverHandler { id: hover }

        // Body click → paste (bottom of the interaction stack).
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: card.shell.paste(card.entry.id)
        }

        // Action overlay (declared last → above the paste MouseArea).
        Row {
            anchors.top: parent.top
            anchors.right: parent.right
            anchors.margins: 6
            spacing: 8

            Text {
                text: card.entry.favorite ? "★" : "☆"
                color: card.entry.favorite ? card.shell.theme.favorite : card.muted
                font.pixelSize: 14
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: card.shell.toggleFavorite(card.entry.id)
                }
            }

            Text {
                visible: card.isImage && hover.hovered
                text: "OCR"
                color: card.shell.theme.accent
                font.pixelSize: 11
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: card.shell.runOcr(card.entry.id)
                }
            }

            Text {
                visible: hover.hovered
                text: "✕"
                color: card.shell.theme.danger
                font.pixelSize: 14
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: card.shell.del(card.entry.id)
                }
            }
        }
    }
}
