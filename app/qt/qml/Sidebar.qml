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
    readonly property color hover: th ? th.sideHover : "#12000000"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color topC: th ? th.sidebarTop : "#ececef"
    readonly property color botC: th ? th.sidebarBot : "#e5e5ea"
    readonly property color hair: th ? th.hairline : "#0b000000"
    readonly property int itemH: th ? th.sideItemH : 32
    readonly property int r: th ? th.radius : 6

    function t(k) { return i18 ? i18.t(k) : k }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: root.topC }
            GradientStop { position: 1; color: root.botC }
        }
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
        anchors.leftMargin: collapsed ? 6 : 8
        anchors.rightMargin: collapsed ? 6 : 8
        anchors.topMargin: 2
        spacing: 2

        AbstractButton {
            id: collapse
            width: nav.width
            height: 24
            hoverEnabled: true
            Accessible.name: root.t(collapsed ? "nav.expandTitle" : "nav.collapseTitle")
            ToolTip.visible: hovered
            ToolTip.text: root.t("title.collapse")
            onClicked: if (win) win.sidebarCollapsed = !win.sidebarCollapsed
            background: Rectangle {
                radius: 4
                color: collapse.hovered ? root.hover : "transparent"
                border.width: 1
                border.color: root.sep
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
                width: nav.width
                height: root.itemH
                padding: 0
                hoverEnabled: true
                Accessible.name: root.t(modelData.key)
                Accessible.role: Accessible.MenuItem
                ToolTip.visible: hovered
                ToolTip.text: root.t(modelData.tip)
                ToolTip.delay: 500
                onClicked: if (win) win.currentView = modelData.id
                property bool on: win && win.currentView === modelData.id
                background: Rectangle {
                    radius: root.r
                    color: item.on ? root.blue : (item.hovered ? root.hover : "transparent")
                }
                contentItem: Item {
                    Row {
                        x: root.collapsed ? Math.round((parent.width - width) / 2) : 8
                        y: Math.round((parent.height - height) / 2)
                        spacing: 8
                        Canvas {
                            id: ico
                            width: 16
                            height: 16
                            property string kind: modelData.icon
                            property string iconMode: th ? th.iconStyle : "Monochrome"
                            property color stroke: {
                                if (item.on) return "#ffffff"
                                if (iconMode === "Colorful") {
                                    if (kind === "home") return root.blue
                                    if (kind === "fw") return (th ? th.purple : "#af52de")
                                    if (kind === "sub") return (th ? th.green : "#34c759")
                                    return (th ? th.orange : "#ff9f0a")
                                }
                                return root.secondary
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
                                var s = 16 / 24
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
                            text: root.t(modelData.key)
                            color: item.on ? "#ffffff" : root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            font.weight: Font.Medium
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
            if (!pressed || !win) return
            var w = Math.round(startW + (m.x - startX))
            win.sidebarWidth = Math.max(150, Math.min(200, w))
        }
    }

    Behavior on width { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
}
