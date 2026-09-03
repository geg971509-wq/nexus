pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window

Item {
    id: root
    property bool collapsed: win ? win.sidebarCollapsed : false
    property int expandedW: win ? win.sidebarWidth : 180
    width: collapsed ? 72 : expandedW
    implicitWidth: width

    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
    readonly property color label: th ? th.label : "#1d1d1f"
    readonly property color secondary: th ? th.secondary : "#6e6e73"
    readonly property color blue: th ? th.blue : "#007aff"
    readonly property color icon: th ? th.icon : secondary
    readonly property color hover: th ? th.sideHover : "#12000000"
    readonly property color pressed: th ? th.pressed : hover
    readonly property color selectionSoft: th ? th.selectionSoft : "#290a84ff"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color sidebarBg: th ? th.sidebarBackground : "#f5f5f7"
    readonly property color hair: th ? th.hairline : "#0b000000"
    readonly property int itemH: th ? th.sideItemH : 32
    readonly property int r: th ? th.radius : 6

    function t(k) { return i18 ? i18.t(k) : k }

    Rectangle {
        anchors.fill: parent
        color: root.sidebarBg
    }
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        width: 1
        color: root.sep
    }

    Column {
        id: nav
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: root.collapsed ? 6 : 8
        anchors.rightMargin: root.collapsed ? 6 : 8
        // In the collapsed macOS rail the traffic lights occupy the top-left
        // chrome. Keep the expand control below that 48 px titlebar zone.
        anchors.topMargin: root.collapsed && Qt.platform.os === "osx" ? 48 : 8
        spacing: 3

        AbstractButton {
            id: collapse
            width: root.collapsed ? nav.width : 32
            height: 32
            x: root.collapsed ? 0 : nav.width - width
            hoverEnabled: true
            focusPolicy: Qt.StrongFocus
            Accessible.name: root.t(root.collapsed ? "nav.expandTitle" : "nav.collapseTitle")
            Tip { text: root.t("title.collapse") }
            onClicked: if (root.win) root.win.sidebarCollapsed = !root.win.sidebarCollapsed
            background: Rectangle {
                radius: root.r
                color: collapse.down ? root.pressed : (collapse.hovered ? root.hover : "transparent")
                border.width: collapse.activeFocus ? 1 : 0
                border.color: root.blue
            }
            contentItem: Item {
                Canvas {
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.clearRect(0, 0, 14, 14)
                        ctx.strokeStyle = root.secondary
                        ctx.lineWidth = 1.2
                        ctx.strokeRect(1, 1, 12, 12)
                        ctx.beginPath()
                        ctx.moveTo(root.collapsed ? 5 : 8, 1)
                        ctx.lineTo(root.collapsed ? 5 : 8, 13)
                        ctx.stroke()
                    }
                }
            }
        }

        Repeater {
            model: [
                { id: "home", icon: "home", key: "nav.home", tip: "title.navHome" },
                { id: "firewall", icon: "fw", key: "nav.firewall", tip: "title.navFirewall" },
                { id: "sub", icon: "sub", key: "nav.sub", tip: "title.navSub" },
                { id: "settings", icon: "set", key: "nav.settings", tip: "title.navSettings" }
            ]
            delegate: AbstractButton {
                id: item
                required property var modelData
                width: nav.width
                height: root.itemH
                padding: 0
                hoverEnabled: true
                checkable: true
                autoExclusive: true
                focusPolicy: Qt.StrongFocus
                Accessible.name: root.t(item.modelData.key)
                Accessible.role: Accessible.RadioButton
                Accessible.checkable: true
                Accessible.checked: item.on
                Tip {
                    text: root.t(item.modelData.tip)
                    active: item.hovered && root.collapsed
                    delay: 500
                }
                onClicked: if (root.win) root.win.currentView = item.modelData.id
                property bool on: root.win && root.win.currentView === item.modelData.id
                checked: on
                onOnChanged: if (checked !== on) checked = on
                background: Rectangle {
                    radius: root.r
                    color: item.on ? root.selectionSoft
                                   : (item.down ? root.pressed : (item.hovered ? root.hover : "transparent"))
                    border.width: item.activeFocus ? 1 : 0
                    border.color: root.blue
                    Behavior on color { ColorAnimation { duration: 160 } }
                }
                contentItem: Item {
                    Row {
                        x: root.collapsed ? Math.round((parent.width - width) / 2) : 10
                        y: Math.round((parent.height - height) / 2)
                        spacing: 8
                        Canvas {
                            id: ico
                            width: 18
                            height: 18
                            property string kind: item.modelData.icon
                            property string iconMode: root.th ? root.th.iconStyle : "Monochrome"
                            property color stroke: {
                                if (item.on) return root.blue
                                if (iconMode === "Colorful") {
                                    if (kind === "home") return root.blue
                                    if (kind === "fw") return (root.th ? root.th.purple : "#af52de")
                                    if (kind === "sub") return (root.th ? root.th.green : "#34c759")
                                    return (root.th ? root.th.orange : "#ff9f0a")
                                }
                                return root.icon
                            }
                            onStrokeChanged: requestPaint()
                            onKindChanged: requestPaint()
                            onPaint: {
                                var ctx = getContext("2d")
                                ctx.clearRect(0, 0, 16, 16)
                                ctx.strokeStyle = stroke
                                ctx.lineWidth = 1.4
                                ctx.lineCap = "round"
                                ctx.lineJoin = "round"
                                var s = 18 / 24
                                ctx.save()
                                ctx.scale(s, s)
                                ctx.beginPath()
                                if (kind === "home") {
                                    rrect(ctx, 3.5, 3.5, 7, 7, 1.6)
                                    rrect(ctx, 13.5, 3.5, 7, 7, 1.6)
                                    rrect(ctx, 3.5, 13.5, 7, 7, 1.6)
                                    rrect(ctx, 13.5, 13.5, 7, 7, 1.6)
                                } else if (kind === "fw") {
                                    ctx.arc(12, 12, 8.25, 0, Math.PI * 2)
                                    ctx.moveTo(6.2, 6.2); ctx.lineTo(17.8, 17.8)
                                } else if (kind === "sub") {
                                    ctx.moveTo(4.5, 7.5); ctx.lineTo(19.5, 7.5)
                                    ctx.moveTo(4.5, 12); ctx.lineTo(19.5, 12)
                                    ctx.moveTo(4.5, 16.5); ctx.lineTo(14.5, 16.5)
                                    ctx.moveTo(16.5, 15); ctx.lineTo(19, 17.5); ctx.lineTo(22, 14)
                                } else {
                                    ctx.arc(12, 12, 3.1, 0, Math.PI * 2)
                                    ctx.moveTo(12, 3.2)
                                    for (var i = 0; i < 8; i++) {
                                        var a = (i / 8) * Math.PI * 2 - Math.PI / 2
                                        ctx.lineTo(12 + Math.cos(a) * 8.4, 12 + Math.sin(a) * 8.4)
                                    }
                                    ctx.closePath()
                                }
                                ctx.stroke()
                                ctx.restore()
                            }
                            function rrect(ctx, x, y, w, h, r) {
                                ctx.moveTo(x + r, y)
                                ctx.arcTo(x + w, y, x + w, y + h, r)
                                ctx.arcTo(x + w, y + h, x, y + h, r)
                                ctx.arcTo(x, y + h, x, y, r)
                                ctx.arcTo(x, y, x + w, y, r)
                            }
                        }
                        Text {
                            visible: !root.collapsed
                            text: root.t(item.modelData.key)
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            font.weight: item.on ? Font.DemiBold : Font.Normal
                            anchors.verticalCenter: parent.verticalCenter
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }

    MouseArea {
        id: resizer
        visible: !root.collapsed
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        width: 6
        cursorShape: Qt.SplitHCursor
        property int startW: 180
        property real startX: 0
        Accessible.name: root.t("title.sideResize")
        onPressed: function(m) {
            startW = root.expandedW
            startX = m.x
        }
        onPositionChanged: function(m) {
            if (!pressed || !root.win) return
            var w = Math.round(startW + (m.x - startX))
            root.win.sidebarWidth = Math.max(150, Math.min(200, w))
        }
    }

    Behavior on width { NumberAnimation { duration: root.th ? root.th.fastAnimation : 160; easing.type: Easing.OutCubic } }
}
