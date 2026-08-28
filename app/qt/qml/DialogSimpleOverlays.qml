import QtQuick
import QtQuick.Controls

Item {
    id: layer

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
        host: layer.host
        id: quitMask
        DialogCard {
            host: layer.host
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
                    text: layer.host.t("quit.title")
                    color: layer.host.orange
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: layer.host.t("quit.msg")
                    color: layer.host.secondary
                    wrapMode: Text.WordWrap
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 13
                }
                Item { width: 1; height: 6 }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: layer.host; text: layer.host.t("btn.cancel"); onClicked: quitMask.visible = false }
                    DialogButton { host: layer.host; text: layer.host.t("quit.confirm"); primary: true; onClicked: layer.host.confirmQuit() }
                }
            }
        }
    }

    DialogMask {
        host: layer.host
        id: statsMask
        DialogCard {
            host: layer.host
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
                    text: layer.host.t("stats.title")
                    color: layer.host.label
                    font.family: layer.host.fonts[0]
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
                            { k: "dock.conn", v: "conn" },
                            { k: "Proxy", v: "proxy" },
                            { k: "Direct", v: "direct" },
                            { k: "stats.uptime", v: "uptime" },
                            { k: "stats.exitIp", v: "ip" },
                            { k: "stats.country", v: "country" },
                            { k: "stats.nextSub", v: "next" }
                        ]
                        delegate: Column {
                            required property var modelData
                            width: (stCol.width - 12) / 2
                            spacing: 2
                            Text {
                                text: modelData.k.indexOf(".") >= 0 ? layer.host.t(modelData.k) : modelData.k
                                color: layer.host.tertiary
                                font.family: layer.host.fonts[0]
                                font.pixelSize: 11
                            }
                            Text {
                                text: String((layer.host.stats && layer.host.stats[modelData.v]) || "—")
                                color: layer.host.label
                                font.family: layer.host.fonts[0]
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
                    DialogButton { host: layer.host; text: layer.host.t("btn.refresh"); onClicked: { layer.host.fillStats(); layer.host.log("SYS", "info", layer.host.t("log.runtimeRefreshed")) } }
                    DialogButton { host: layer.host; text: layer.host.t("btn.close"); primary: true; onClicked: statsMask.visible = false }
                }
            }
        }
    }

    DialogMask {
        host: layer.host
        id: exportMask
        DialogCard {
            host: layer.host
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
                    text: layer.host.t("export.title")
                    color: layer.host.label
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: layer.host.t("export.sub")
                    color: layer.host.secondary
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 13
                }
                ScrollView {
                    width: parent.width
                    height: 220
                    TextArea {
                        text: layer.host.exportText
                        readOnly: true
                        wrapMode: TextEdit.Wrap
                        color: layer.host.label
                        font.family: layer.host.mono[0]
                        font.pixelSize: 11
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: layer.host; text: layer.host.t("btn.close"); onClicked: exportMask.visible = false }
                    DialogButton { host: layer.host; text: layer.host.t("btn.copyAll"); primary: true; onClicked: layer.host.copyExport() }
                }
            }
        }
    }

    DialogMask {
        host: layer.host
        id: askMask
        z: 420
        dismiss: function () { layer.host.closeAsk(false) }
        DialogCard {
            host: layer.host
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
                    text: layer.host.askTitle
                    color: layer.host.askDanger ? layer.host.orange : layer.host.label
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: layer.host.askMsg
                    color: layer.host.secondary
                    wrapMode: Text.WordWrap
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 13
                }
                Item { width: 1; height: 6 }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton {
                        host: layer.host
                        text: layer.host.t("btn.cancel")
                        uniform: layer.host.askUniform
                        onClicked: layer.host.closeAsk(false)
                    }
                    DialogButton {
                        host: layer.host
                        text: layer.host.askOkText
                        uniform: layer.host.askUniform
                        primary: !layer.host.askDanger
                        danger: layer.host.askDanger
                        onClicked: layer.host.closeAsk(true)
                    }
                }
            }
        }
    }

    DialogMask {
        host: layer.host
        id: qrMask
        DialogCard {
            host: layer.host
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
                    text: layer.host.t("qr.title")
                    color: layer.host.label
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    text: layer.host.qrName
                    color: layer.host.secondary
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 13
                    elide: Text.ElideRight
                    width: parent.width
                }
                Item {
                    width: parent.width
                    height: 228
                    Image {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: layer.host.qrSvg.length > 0
                        source: layer.host.qrSvg.length ? ("data:image/svg+xml;utf8," + encodeURIComponent(layer.host.qrSvg)) : ""
                        sourceSize.width: 220
                        sourceSize.height: 220
                        fillMode: Image.PreserveAspectFit
                        width: 220
                        height: 220
                    }
                    Text {
                        visible: !layer.host.qrSvg.length
                        anchors.centerIn: parent
                        text: layer.host.qrLink ? layer.host.t("js.generating") : layer.host.t("qr.empty")
                        color: layer.host.tertiary
                        font.family: layer.host.fonts[0]
                        font.pixelSize: 13
                    }
                }
                Text {
                    width: parent.width
                    text: layer.host.qrLink || layer.host.t("qr.noShare")
                    color: layer.host.secondary
                    wrapMode: Text.WrapAnywhere
                    font.family: layer.host.mono[0]
                    font.pixelSize: 11
                    maximumLineCount: 6
                    elide: Text.ElideRight
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: layer.host; text: layer.host.t("btn.close"); onClicked: qrMask.visible = false }
                    DialogButton { host: layer.host; text: layer.host.t("ctx.copyLink"); primary: true; onClicked: layer.host.copyQr() }
                }
            }
        }
    }

}
