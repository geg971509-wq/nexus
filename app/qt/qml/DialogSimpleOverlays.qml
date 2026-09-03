pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

Item {
    id: root

    required property var host
    width: 0
    height: 0

    function showQuit() { quitMask.visible = true }
    function hideQuit() { quitMask.visible = false }
    function showStats() { statsMask.visible = true }
    function showExport() { exportMask.visible = true }
    function showConfirm() { askMask.visible = true }
    function hideConfirm() { askMask.visible = false }
    function showQr() { qrMask.visible = true }

    DialogMask {
        host: root.host
        id: quitMask
        DialogCard {
            host: root.host
            cardW: 360
            implicitHeight: qCol.implicitHeight + 32
            Column {
                id: qCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.host.t("quit.title")
                    color: root.host.orange
                    font.family: root.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: root.host.t("quit.msg")
                    color: root.host.secondary
                    wrapMode: Text.WordWrap
                    font.family: root.host.fonts[0]
                    font.pixelSize: 13
                }
                Item { width: 1; height: 6 }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: root.host; text: root.host.t("btn.cancel"); onClicked: quitMask.visible = false }
                    DialogButton { host: root.host; text: root.host.t("quit.confirm"); primary: true; onClicked: root.host.confirmQuit() }
                }
            }
        }
    }

    DialogMask {
        host: root.host
        id: statsMask
        DialogCard {
            host: root.host
            cardW: 440
            implicitHeight: stCol.implicitHeight + 32
            Column {
                id: stCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 10
                Text {
                    text: root.host.t("stats.title")
                    color: root.host.label
                    font.family: root.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Grid {
                    width: parent.width
                    columns: 2
                    columnSpacing: 12
                    rowSpacing: 10
                    Repeater {
                        model: [
                            { k: "stats.core", v: "core" },
                            { k: "Proxy", v: "proxy" },
                            { k: "stats.uptime", v: "uptime" },
                            { k: "stats.exitIp", v: "ip" },
                            { k: "stats.country", v: "country" },
                            { k: "stats.nextSub", v: "next" }
                        ]
                        delegate: Column {
                            id: statCell
                            required property var modelData
                            width: (stCol.width - 12) / 2
                            spacing: 2
                            Text {
                                text: statCell.modelData.k.indexOf(".") >= 0
                                      ? root.host.t(statCell.modelData.k)
                                      : statCell.modelData.k
                                color: root.host.tertiary
                                font.family: root.host.fonts[0]
                                font.pixelSize: 11
                            }
                            Text {
                                text: String((root.host.stats && root.host.stats[statCell.modelData.v]) || "—")
                                color: root.host.label
                                font.family: root.host.fonts[0]
                                font.pixelSize: 13
                                wrapMode: Text.Wrap
                                width: parent.width
                            }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: root.host; text: root.host.t("btn.refresh"); onClicked: { root.host.fillStats(); root.host.log("SYS", "info", root.host.t("log.runtimeRefreshed")) } }
                    DialogButton { host: root.host; text: root.host.t("btn.close"); primary: true; onClicked: statsMask.visible = false }
                }
            }
        }
    }

    DialogMask {
        host: root.host
        id: exportMask
        DialogCard {
            host: root.host
            cardW: 440
            implicitHeight: Math.min(480, exCol.implicitHeight + 32)
            Column {
                id: exCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.host.t("export.title")
                    color: root.host.label
                    font.family: root.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: root.host.t("export.sub")
                    color: root.host.secondary
                    font.family: root.host.fonts[0]
                    font.pixelSize: 13
                }
                ScrollView {
                    width: parent.width
                    height: 220
                    TextArea {
                        text: root.host.exportText
                        readOnly: true
                        wrapMode: TextEdit.Wrap
                        color: root.host.label
                        font.family: root.host.mono[0]
                        font.pixelSize: 11
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: root.host; text: root.host.t("btn.close"); onClicked: exportMask.visible = false }
                    DialogButton { host: root.host; text: root.host.t("btn.copyAll"); primary: true; onClicked: root.host.copyExport() }
                }
            }
        }
    }

    DialogMask {
        host: root.host
        id: askMask
        z: 420
        dismiss: function () { root.host.closeAsk(false) }
        DialogCard {
            host: root.host
            cardW: 360
            implicitHeight: askCol.implicitHeight + 32
            Column {
                id: askCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.host.askTitle
                    color: root.host.askDanger ? root.host.orange : root.host.label
                    font.family: root.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: root.host.askMsg
                    color: root.host.secondary
                    wrapMode: Text.WordWrap
                    font.family: root.host.fonts[0]
                    font.pixelSize: 13
                }
                Item { width: 1; height: 6 }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton {
                        host: root.host
                        text: root.host.t("btn.cancel")
                        uniform: root.host.askUniform
                        onClicked: root.host.closeAsk(false)
                    }
                    DialogButton {
                        host: root.host
                        text: root.host.askOkText
                        uniform: root.host.askUniform
                        primary: !root.host.askDanger
                        danger: root.host.askDanger
                        onClicked: root.host.closeAsk(true)
                    }
                }
            }
        }
    }

    DialogMask {
        host: root.host
        id: qrMask
        DialogCard {
            host: root.host
            cardW: 380
            implicitHeight: qrCol.implicitHeight + 32
            Column {
                id: qrCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.host.t("qr.title")
                    color: root.host.label
                    font.family: root.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    text: root.host.qrName
                    color: root.host.secondary
                    font.family: root.host.fonts[0]
                    font.pixelSize: 13
                    elide: Text.ElideRight
                    width: parent.width
                }
                Item {
                    width: parent.width
                    height: 228
                    Image {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: root.host.qrSvg.length > 0
                        source: root.host.qrSvg.length ? ("data:image/svg+xml;utf8," + encodeURIComponent(root.host.qrSvg)) : ""
                        sourceSize.width: 220
                        sourceSize.height: 220
                        fillMode: Image.PreserveAspectFit
                        width: 220
                        height: 220
                    }
                    Text {
                        visible: !root.host.qrSvg.length
                        anchors.centerIn: parent
                        text: root.host.qrLink ? root.host.t("js.generating") : root.host.t("qr.empty")
                        color: root.host.tertiary
                        font.family: root.host.fonts[0]
                        font.pixelSize: 13
                    }
                }
                Text {
                    width: parent.width
                    text: root.host.qrLink || root.host.t("qr.noShare")
                    color: root.host.secondary
                    wrapMode: Text.WrapAnywhere
                    font.family: root.host.mono[0]
                    font.pixelSize: 11
                    maximumLineCount: 6
                    elide: Text.ElideRight
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: root.host; text: root.host.t("btn.close"); onClicked: qrMask.visible = false }
                    DialogButton { host: root.host; text: root.host.t("ctx.copyLink"); primary: true; onClicked: root.host.copyQr() }
                }
            }
        }
    }

}
