import QtQuick
import QtQuick.Controls

Item {
    id: layer

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
        onClicked: layer.host.ctxAct(ctxItem.act)
        background: Rectangle {
            radius: 4
            color: ctxItem.hovered && ctxItem.enabled ? layer.host.fill : "transparent"
        }
        contentItem: Text {
            text: ctxItem.text
            color: ctxItem.enabled ? layer.host.label : layer.host.tertiary
            font.family: layer.host.fonts[0]
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
            color: layer.host.menuBg
            border.color: layer.host.menuBorder
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
            var t = layer.host.home && layer.host.home.table
            if (!t) return false
            var names = t.selectedNames
            if (names && names.length) return true
            return !!(t.selectedName && t.selectedName !== "—")
        }
        readonly property bool hasUrl: {
            var data = layer.host.catalog || (layer.host.home && layer.host.home.catalog)
            if (!data || !data.groups || !data.groups.length) return false
            var id = (layer.host.home && layer.host.home.activeGid) || (layer.host.win && layer.host.win.subTab) || data.active || "default"
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
                CtxItem { text: layer.host.t("ctx.addClip"); act: "add-clip" }
                CtxItem { text: layer.host.t("ctx.addFile"); act: "add-file" }
                CtxItem { text: layer.host.t("menu.scanQr"); act: "scan-qr" }
                CtxSep {}
                CtxItem { text: layer.host.t("ctx.edit"); act: "edit"; enabled: nodeCtx.hasSel }
                CtxItem { text: layer.host.t("ctx.start"); act: "start"; enabled: nodeCtx.hasSel }
                CtxItem { text: layer.host.t("ctx.stop"); act: "stop"; enabled: !!(layer.host.home && layer.host.home.connected) }
                CtxItem { text: layer.host.t("ctx.clone"); act: "clone"; enabled: nodeCtx.hasSel }
                CtxItem { text: layer.host.t("ctx.delete"); act: "delete"; enabled: nodeCtx.hasSel }
                CtxSep {}
                CtxItem { text: layer.host.t("ctx.copyLink"); act: "copy-link"; enabled: nodeCtx.hasSel }
                CtxItem { text: layer.host.t("ctx.showQr"); act: "show-qr"; enabled: nodeCtx.hasSel }
                CtxSep {}
                CtxItem { text: layer.host.t("ctx.selectAll"); act: "select-all" }
                CtxItem { text: layer.host.t("ctx.refreshSub"); act: "refresh-sub"; enabled: nodeCtx.hasUrl }
                CtxSep {}
                CtxItem { text: layer.host.t("test.urlSelected"); act: "url-test"; enabled: nodeCtx.hasSel }
                CtxItem { text: layer.host.t("ctx.resolveIp"); act: "resolve-ip"; enabled: nodeCtx.hasSel }
                CtxItem { text: layer.host.t("test.clear"); act: "clear-test" }
                CtxItem { text: layer.host.t("ctx.resetTraffic"); act: "reset-traffic" }
                CtxSep {}
                CtxItem { text: layer.host.t("ctx.dedupe"); act: "dedupe" }
                CtxItem { text: layer.host.t("ctx.rmUnavailable"); act: "rm-unavailable" }
                CtxItem { text: layer.host.t("ctx.rmFailed"); act: "rm-failed" }
            }
        }
    }

    DialogMask {
        host: layer.host
        id: editMask
        DialogCard {
            host: layer.host
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
                    text: layer.host.t("edit.title")
                    color: layer.host.label
                    font.family: layer.host.fonts[0]
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
                            text: layer.host.eName
                            placeholderText: layer.host.t("edit.name")
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eName = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        Row {
                            width: parent.width
                            spacing: 8
                            ComboBox {
                                id: eTypeBox
                                width: (parent.width - 8) / 2
                                model: ["VLESS", "Trojan", "SS", "VMess", "HTTP", "HTTPS", "SOCKS", "AnyTLS", "TUIC"]
                                onActivated: layer.host.eType = currentText
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: layer.host.ePort
                                placeholderText: layer.host.t("edit.port")
                                color: layer.host.label
                                font.family: layer.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: layer.host.ePort = text
                                background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                            }
                        }
                        TextField {
                            width: parent.width
                            text: layer.host.eServer
                            placeholderText: layer.host.t("edit.server")
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eServer = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        TextField {
                            visible: layer.host.eShow("vless vmess tuic")
                            width: parent.width
                            text: layer.host.eUuid
                            placeholderText: "UUID"
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eUuid = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        TextField {
                            visible: layer.host.eShow("vless")
                            width: parent.width
                            text: layer.host.eFlow
                            placeholderText: "Flow"
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eFlow = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        Row {
                            visible: layer.host.eShow("vmess")
                            width: parent.width
                            spacing: 8
                            TextField {
                                width: (parent.width - 8) / 2
                                text: layer.host.eSecurity
                                placeholderText: layer.host.t("edit.security")
                                color: layer.host.label
                                font.family: layer.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: layer.host.eSecurity = text
                                background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: layer.host.eAlterId
                                placeholderText: "AlterID"
                                color: layer.host.label
                                font.family: layer.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: layer.host.eAlterId = text
                                background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                            }
                        }
                        TextField {
                            visible: layer.host.eShow("http https socks")
                            width: parent.width
                            text: layer.host.eUser
                            placeholderText: layer.host.t("edit.user")
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eUser = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        TextField {
                            visible: layer.host.eShow("trojan ss http https socks anytls tuic")
                            width: parent.width
                            text: layer.host.ePass
                            placeholderText: layer.host.t("edit.pass")
                            echoMode: TextInput.Password
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.ePass = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        TextField {
                            visible: layer.host.eShow("ss")
                            width: parent.width
                            text: layer.host.eMethod
                            placeholderText: layer.host.t("edit.method")
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eMethod = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        TextField {
                            visible: layer.host.eShow("tuic")
                            width: parent.width
                            text: layer.host.eCongest
                            placeholderText: layer.host.t("edit.congest")
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eCongest = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        TextField {
                            visible: layer.host.eShow("tuic")
                            width: parent.width
                            text: layer.host.eAlpn
                            placeholderText: "ALPN"
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eAlpn = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        TextField {
                            visible: layer.host.eShow("vless vmess trojan https anytls tuic")
                            width: parent.width
                            text: layer.host.eSni
                            placeholderText: layer.host.t("edit.sni")
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eSni = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        Row {
                            visible: layer.host.eShow("vless vmess trojan")
                            width: parent.width
                            spacing: 8
                            TextField {
                                width: (parent.width - 8) / 2
                                text: layer.host.eNetwork
                                placeholderText: layer.host.t("edit.network")
                                color: layer.host.label
                                font.family: layer.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: layer.host.eNetwork = text
                                background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: layer.host.eHost
                                placeholderText: layer.host.t("edit.host")
                                color: layer.host.label
                                font.family: layer.host.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: layer.host.eHost = text
                                background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                            }
                        }
                        TextField {
                            visible: layer.host.eShow("vless vmess trojan http https")
                            width: parent.width
                            text: layer.host.ePath
                            placeholderText: layer.host.t("edit.path")
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.ePath = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                        CheckBox {
                            visible: layer.host.eShow("vless vmess trojan https anytls")
                            text: layer.host.t("edit.tls")
                            checked: layer.host.eTls
                            onToggled: layer.host.eTls = checked
                        }
                        CheckBox {
                            visible: layer.host.eShow("vless vmess trojan https anytls tuic")
                            text: layer.host.t("edit.insecure")
                            checked: layer.host.eInsecure
                            onToggled: layer.host.eInsecure = checked
                        }
                        TextField {
                            width: parent.width
                            text: layer.host.eNote
                            placeholderText: layer.host.t("edit.note")
                            color: layer.host.label
                            font.family: layer.host.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: layer.host.eNote = text
                            background: Rectangle { radius: 8; color: layer.host.menuBg; border.width: 1; border.color: layer.host.menuBorder }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DialogButton { host: layer.host; text: layer.host.t("btn.cancel"); onClicked: editMask.visible = false }
                    DialogButton { host: layer.host; text: layer.host.t("btn.save"); primary: true; onClicked: layer.host.saveEdit() }
                }
            }
        }
    }

}
