pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

Item {
    id: root

    required property var host
    readonly property var th: host.th
    width: 0
    height: 0

    function showEdit() { editMask.visible = true }
    function hideEdit() { editMask.visible = false }
    function selectEditType(index) { eTypeBox.currentIndex = index }
    function openContext(gx, gy) {
        var overlay = Overlay.overlay
        var point = overlay ? overlay.mapFromGlobal(gx, gy) : Qt.point(gx, gy)
        nodeCtx.selCount = host.tableSel().length
        nodeCtx.placeAt(point.x, point.y)
        nodeCtx.open()
    }
    function closeContext() { nodeCtx.close() }

    component CtxItem: AbstractButton {
        id: ctxItem
        property string act: ""
        width: parent ? parent.width : 220
        height: 28
        hoverEnabled: true
        onClicked: root.host.ctxAct(ctxItem.act)
        background: Rectangle {
            radius: 4
            color: ctxItem.hovered && ctxItem.enabled ? root.host.fill : "transparent"
        }
        contentItem: Text {
            text: ctxItem.text
            color: ctxItem.enabled ? root.host.label : root.host.tertiary
            font.family: root.host.fonts[0]
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
            leftPadding: 8
            opacity: ctxItem.enabled ? 1 : 0.45
        }
    }

    component CtxSep: Item {
        width: parent ? parent.width : 220
        height: 9
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 6
            height: 1
            color: th ? th.separator : "#1e3c3c43"
        }
    }

    Popup {
        id: nodeCtx
        parent: Overlay.overlay
        padding: 6
        width: 232
        height: Math.min(naturalHeight, Math.max(1, (parent ? parent.height : naturalHeight) - 16))
        modal: false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle {
            radius: 12
            color: root.host.menuBg
            border.color: root.host.menuBorder
            border.width: 1
        }
        property int selCount: 0
        property real anchorX: 0
        property real anchorY: 0
        readonly property real naturalHeight: nodeCtxColumn.implicitHeight + topPadding + bottomPadding
        function placeAt(px, py) {
            anchorX = px
            anchorY = py
            reposition()
        }
        function reposition() {
            var ov = parent
            if (!ov) { x = anchorX; y = anchorY; return }
            var margin = 8
            var nx = anchorX
            var ny = anchorY
            if (nx + width + margin > ov.width) nx = anchorX - width
            if (ny + height + margin > ov.height) ny = anchorY - height
            x = Math.max(margin, Math.min(nx, Math.max(margin, ov.width - width - margin)))
            y = Math.max(margin, Math.min(ny, Math.max(margin, ov.height - height - margin)))
        }
        onOpened: reposition()
        onHeightChanged: if (visible) reposition()
        Connections {
            target: nodeCtx.parent
            function onWidthChanged() { if (nodeCtx.visible) nodeCtx.reposition() }
        }
        readonly property bool hasSel: {
            if (selCount > 0) return true
            var t = root.host.home && root.host.home.table
            if (!t) return false
            var names = t.selectedNames
            if (names && names.length) return true
            return !!(t.selectedName && t.selectedName !== "—")
        }
        readonly property bool hasUrl: {
            var data = root.host.catalog || (root.host.home && root.host.home.catalog)
            if (!data || !data.groups || !data.groups.length) return false
            var id = (root.host.home && root.host.home.activeGid) || (root.host.win && root.host.win.subTab) || data.active || "default"
            var g = null
            for (var i = 0; i < data.groups.length; i++)
                if (data.groups[i].id === id) { g = data.groups[i]; break }
            if (!g) g = data.groups[0]
            return !!(g && g.url)
        }
        contentItem: Flickable {
            id: nodeCtxScroll
            clip: true
            contentWidth: width
            contentHeight: nodeCtxColumn.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick
            ScrollBar.vertical: ScrollBar {
                policy: nodeCtxScroll.contentHeight > nodeCtxScroll.height
                    ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
            }
            Column {
                id: nodeCtxColumn
                width: nodeCtxScroll.width
                spacing: 0
                CtxItem { text: root.host.t("ctx.addClip"); act: "add-clip" }
                CtxItem { text: root.host.t("ctx.addFile"); act: "add-file" }
                CtxItem { text: root.host.t("menu.scanQr"); act: "scan-qr" }
                CtxSep {}
                CtxItem { text: root.host.t("ctx.edit"); act: "edit"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.host.t("ctx.start"); act: "start"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.host.t("ctx.stop"); act: "stop"; enabled: !!(root.host.home && root.host.home.connected) }
                CtxItem { text: root.host.t("ctx.clone"); act: "clone"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.host.t("ctx.delete"); act: "delete"; enabled: nodeCtx.hasSel }
                CtxSep {}
                CtxItem { text: root.host.t("ctx.copyLink"); act: "copy-link"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.host.t("ctx.showQr"); act: "show-qr"; enabled: nodeCtx.hasSel }
                CtxSep {}
                CtxItem { text: root.host.t("ctx.selectAll"); act: "select-all" }
                CtxItem { text: root.host.t("ctx.refreshSub"); act: "refresh-sub"; enabled: nodeCtx.hasUrl }
                CtxSep {}
                CtxItem { text: root.host.t("test.urlSelected"); act: "url-test"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.host.t("ctx.resolveIp"); act: "resolve-ip"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.host.t("test.clear"); act: "clear-test" }
                CtxItem { text: root.host.t("ctx.resetTraffic"); act: "reset-traffic" }
                CtxSep {}
                CtxItem { text: root.host.t("ctx.dedupe"); act: "dedupe" }
                CtxItem { text: root.host.t("ctx.rmUnavailable"); act: "rm-unavailable" }
                CtxItem { text: root.host.t("ctx.rmFailed"); act: "rm-failed" }
            }
        }
    }

    DialogMask {
        host: root.host
        id: editMask
        DialogCard {
            host: root.host
            cardW: 480
            implicitHeight: Math.min(560, edCol.implicitHeight + 32)
            Column {
                id: edCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.host.t("edit.title")
                    color: root.host.label
                    font.family: root.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                ScrollView {
                    width: parent.width
                    height: Math.min(420, edForm.implicitHeight + 8)
                    clip: true
                    Column {
                        id: edForm
                        width: edCol.width
                        spacing: 8
                        TextField {
                            width: parent.width
                            text: root.host.eName
                            placeholderText: root.host.t("edit.name")
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eName = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        Row {
                            width: parent.width
                            spacing: 8
                            ComboBox {
                                id: eTypeBox
                                width: (parent.width - 8) / 2
                                model: ["VLESS", "Trojan", "SS", "VMess", "HTTP", "HTTPS", "SOCKS", "AnyTLS", "TUIC"]
                                onActivated: root.host.eType = currentText
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.host.ePort
                                placeholderText: root.host.t("edit.port")
                                color: root.host.label
                                font.family: root.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.host.ePort = text
                                background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                            }
                        }
                        TextField {
                            width: parent.width
                            text: root.host.eServer
                            placeholderText: root.host.t("edit.server")
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eServer = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        TextField {
                            visible: root.host.eShow("vless vmess tuic")
                            width: parent.width
                            text: root.host.eUuid
                            placeholderText: "UUID"
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eUuid = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        TextField {
                            visible: root.host.eShow("vless")
                            width: parent.width
                            text: root.host.eFlow
                            placeholderText: "Flow"
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eFlow = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        Row {
                            visible: root.host.eShow("vmess")
                            width: parent.width
                            spacing: 8
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.host.eSecurity
                                placeholderText: root.host.t("edit.security")
                                color: root.host.label
                                font.family: root.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.host.eSecurity = text
                                background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.host.eAlterId
                                placeholderText: "AlterID"
                                color: root.host.label
                                font.family: root.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.host.eAlterId = text
                                background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                            }
                        }
                        TextField {
                            visible: root.host.eShow("http https socks")
                            width: parent.width
                            text: root.host.eUser
                            placeholderText: root.host.t("edit.user")
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eUser = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        TextField {
                            visible: root.host.eShow("trojan ss http https socks anytls tuic")
                            width: parent.width
                            text: root.host.ePass
                            placeholderText: root.host.t("edit.pass")
                            echoMode: TextInput.Password
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.ePass = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        TextField {
                            visible: root.host.eShow("ss")
                            width: parent.width
                            text: root.host.eMethod
                            placeholderText: root.host.t("edit.method")
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eMethod = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        TextField {
                            visible: root.host.eShow("tuic")
                            width: parent.width
                            text: root.host.eCongest
                            placeholderText: root.host.t("edit.congest")
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eCongest = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        TextField {
                            visible: root.host.eShow("tuic")
                            width: parent.width
                            text: root.host.eAlpn
                            placeholderText: "ALPN"
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eAlpn = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        TextField {
                            visible: root.host.eShow("vless vmess trojan https anytls tuic")
                            width: parent.width
                            text: root.host.eSni
                            placeholderText: root.host.t("edit.sni")
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eSni = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        Row {
                            visible: root.host.eShow("vless vmess trojan")
                            width: parent.width
                            spacing: 8
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.host.eNetwork
                                placeholderText: root.host.t("edit.network")
                                color: root.host.label
                                font.family: root.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.host.eNetwork = text
                                background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.host.eHost
                                placeholderText: root.host.t("edit.host")
                                color: root.host.label
                                font.family: root.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.host.eHost = text
                                background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                            }
                        }
                        TextField {
                            visible: root.host.eShow("vless vmess trojan http https")
                            width: parent.width
                            text: root.host.ePath
                            placeholderText: root.host.t("edit.path")
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.ePath = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                        CheckBox {
                            visible: root.host.eShow("vless vmess trojan https anytls")
                            text: root.host.t("edit.tls")
                            checked: root.host.eTls
                            onToggled: root.host.eTls = checked
                        }
                        CheckBox {
                            visible: root.host.eShow("vless vmess trojan https anytls tuic")
                            text: root.host.t("edit.insecure")
                            checked: root.host.eInsecure
                            onToggled: root.host.eInsecure = checked
                        }
                        TextField {
                            width: parent.width
                            text: root.host.eNote
                            placeholderText: root.host.t("edit.note")
                            color: root.host.label
                            font.family: root.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.host.eNote = text
                            background: Rectangle { radius: 8; color: root.host.menuBg; border.width: 1; border.color: root.host.menuBorder }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: root.host; text: root.host.t("btn.cancel"); onClicked: editMask.visible = false }
                    DialogButton { host: root.host; text: root.host.t("btn.save"); primary: true; onClicked: root.host.saveEdit() }
                }
            }
        }
    }

}

