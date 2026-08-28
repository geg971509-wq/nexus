pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

Item {
    id: root
    implicitHeight: toolsH
    height: implicitHeight

    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property int toolsH: th ? th.titleToolsH : 36
    readonly property int r: th ? th.radius : 6
    readonly property color label: th ? th.label : "#1d1d1f"
    readonly property color secondary: th ? th.secondary : "#6e6e73"
    readonly property color fill: th ? th.fill : "#1e787880"
    readonly property color surface: th ? th.surface : "#ffffff"
    readonly property color heroTop: th ? th.heroTop : "#ffffff"
    readonly property color heroBot: th ? th.heroBot : "#fbfbfd"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color menuBg: th ? th.menuBg : "#ffffff"
    readonly property color menuBorder: th ? th.menuBorder : "#1a000000"
    readonly property color controlBg: th ? th.controlBg : "#ffffff"
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
    readonly property var home: win ? win.home : null
    readonly property var catalog: home ? home.catalog : null
    readonly property var groups: {
        var data = catalog
        if (data && data.groups && data.groups.length)
            return data.groups
        return [
            { id: "default", name: "Default" },
            { id: "backup", name: t("tb.backup") }
        ]
    }

    function t(k) { return i18 ? i18.t(k) : k }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: root.heroTop }
            GradientStop { position: 1; color: root.heroBot }
        }
    }
    Item {
        anchors.fill: parent

        RowLayout {
            id: tools
            anchors.fill: parent
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            spacing: 8

        Flickable {
            id: segFlick
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredHeight: 24
            Layout.preferredWidth: Math.min(Math.max(64, tabRow.implicitWidth + 2), Math.max(64, tools.width - 280))
            Layout.maximumWidth: Math.max(64, tools.width - 280)
            clip: true
            contentWidth: Math.max(width, tabRow.implicitWidth + 2)
            contentHeight: 24
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.HorizontalFlick
            interactive: contentWidth > width + 1

            Rectangle {
                id: seg
                width: Math.max(64, tabRow.implicitWidth + 2)
                height: 24
                radius: root.r
                color: root.controlBg
                border.width: 1
                border.color: root.sep

                Row {
                    id: tabRow
                    x: 1
                    y: 1
                    height: 22
                    spacing: 0
                    Repeater {
                        model: root.groups
                        delegate: AbstractButton {
                            id: tabBtn
                            required property var modelData
                            required property int index
                            readonly property string gid: String((tabBtn.modelData && tabBtn.modelData.id) || "")
                            readonly property string gname: String((tabBtn.modelData && tabBtn.modelData.name) || tabBtn.gid)
                            readonly property bool on: {
                                var home = root.win ? root.win.home : null
                                var cur = (home && home.activeGid) || (root.win && root.win.subTab) || "default"
                                return cur === tabBtn.gid
                            }
                            padding: 0
                            leftPadding: 8
                            rightPadding: 8
                            implicitHeight: 22
                            height: 22
                            implicitWidth: Math.max(40, implicitContentWidth + leftPadding + rightPadding)
                            hoverEnabled: true
                            Accessible.name: gname
                            Accessible.role: Accessible.PageTab
                            ToolTip.visible: hovered
                            ToolTip.text: gname
                            onClicked: {
                                if (root.win && root.win.home && typeof root.win.home.switchGroup === "function")
                                    root.win.home.switchGroup(tabBtn.gid, true)
                                else if (root.win)
                                    root.win.subTab = tabBtn.gid
                            }
                            background: Item {
                                Rectangle {
                                    visible: tabBtn.index > 0
                                    width: 1
                                    height: 12
                                    color: root.sep
                                    anchors.verticalCenter: parent.verticalCenter
                                    x: 0
                                }
                                Rectangle {
                                    anchors.fill: parent
                                    anchors.margins: 1
                                    radius: 4
                                    color: tabBtn.on ? root.fill : (tabBtn.hovered ? root.fill : "transparent")
                                }
                            }
                            contentItem: Text {
                                text: tabBtn.gname
                                color: tabBtn.on ? root.label : root.secondary
                                font.family: root.fonts[0]
                                font.pixelSize: 12
                                font.weight: Font.Medium
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                    }
                }
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            enabled: false
        }

        Row {
            id: actions
            Layout.alignment: Qt.AlignVCenter
            spacing: 6

            AbstractButton {
                id: appBtn
                width: 24
                height: 24
                padding: 0
                hoverEnabled: true
                Accessible.name: root.t("title.appMenu")
                Accessible.role: Accessible.ButtonMenu
                ToolTip.visible: hovered
                ToolTip.text: root.t("title.appMenu")
                onClicked: appMenu.visible ? appMenu.close() : appMenu.open()
                background: Rectangle {
                    radius: root.r
                    color: appBtn.hovered || appMenu.visible ? root.fill : root.controlBg
                    border.width: 1
                    border.color: root.sep
                }
                contentItem: Text {
                    text: "•••"
                    color: root.secondary
                    font.family: root.fonts[0]
                    font.pixelSize: 11
                    font.weight: Font.Medium
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                Popup {
                    id: appMenu
                    y: parent.height + 4
                    x: parent.width - 220
                    width: 220
                    padding: 4
                    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                    background: Rectangle {
                        radius: 6
                        color: root.menuBg
                        border.color: root.menuBorder
                        border.width: 1
                    }
                    contentItem: Column {
                        spacing: 0
                        Repeater {
                            model: [
                                { k: "menu.addClip", tip: "title.addClip", act: "add-clip" },
                                { k: "menu.addFile", tip: "title.addFile", act: "add-file" },
                                { k: "menu.scanQr", tip: "title.scanQr", act: "scan-qr" },
                                { k: "sep" },
                                { k: "menu.export", tip: "title.export", act: "export" },
                                { k: "menu.stats", tip: "title.stats", act: "stats" },
                                { k: "sep" },
                                { k: "menu.hide", tip: "title.hide", act: "hide" },
                                { k: "menu.quit", tip: "title.quit", act: "quit" }
                            ]
                            delegate: Loader {
                                id: appLoader
                                width: 212
                                required property var modelData
                                sourceComponent: appLoader.modelData.k === "sep" ? appSepComp : appRowComp
                                property var itemData: appLoader.modelData
                                onLoaded: if (appLoader.item && "itemData" in appLoader.item) appLoader.item.itemData = appLoader.itemData
                            }
                        }
                    }
                }
            }

            AbstractButton {
                id: testBtn
                height: 24
                padding: 0
                leftPadding: 8
                rightPadding: 8
                implicitWidth: Math.max(24, implicitContentWidth + leftPadding + rightPadding)
                hoverEnabled: true
                Accessible.name: root.t("tb.test")
                ToolTip.visible: hovered
                ToolTip.text: root.t("title.testMenu")
                onClicked: testMenu.visible ? testMenu.close() : testMenu.open()
                background: Rectangle {
                    radius: root.r
                    color: testBtn.hovered || testMenu.visible ? root.fill : root.controlBg
                    border.width: 1
                    border.color: root.sep
                }
                contentItem: Row {
                    spacing: 5
                    Canvas {
                        width: 12; height: 12
                        anchors.verticalCenter: parent.verticalCenter
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.clearRect(0, 0, 12, 12)
                            ctx.strokeStyle = root.secondary
                            ctx.lineWidth = 1.4
                            ctx.lineJoin = "round"
                            ctx.lineCap = "round"
                            ctx.beginPath()
                            ctx.moveTo(6.5, 1)
                            ctx.lineTo(2, 7)
                            ctx.lineTo(5.5, 7)
                            ctx.lineTo(5, 11)
                            ctx.lineTo(9.5, 5)
                            ctx.lineTo(6, 5)
                            ctx.closePath()
                            ctx.stroke()
                        }
                    }
                    Text {
                        text: root.t("tb.test")
                        color: root.label
                        font.family: root.fonts[0]
                        font.pixelSize: 12
                        font.weight: Font.Medium
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                Popup {
                    id: testMenu
                    y: parent.height + 4
                    x: parent.width - 200
                    padding: 4
                    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                    background: Rectangle {
                        radius: 6
                        color: root.menuBg
                        border.color: root.menuBorder
                        border.width: 1
                    }
                    contentItem: Column {
                        spacing: 0
                        Repeater {
                            model: [
                                { k: "test.urlSelected" },
                                { k: "test.urlGroup" },
                                { k: "sep" },
                                { k: "test.stop" },
                                { k: "test.clear" }
                            ]
                            delegate: Loader {
                                id: testLoader
                                width: 188
                                required property var modelData
                                sourceComponent: testLoader.modelData.k === "sep" ? sepComp : rowComp
                                property var itemData: testLoader.modelData
                                onLoaded: if (testLoader.item && "itemData" in testLoader.item) testLoader.item.itemData = testLoader.itemData
                            }
                        }
                    }
                }
            }

            AbstractButton {
                id: groupsBtn
                height: 24
                padding: 0
                leftPadding: 8
                rightPadding: 8
                implicitWidth: Math.max(24, implicitContentWidth + leftPadding + rightPadding)
                hoverEnabled: true
                Accessible.name: root.t("tb.groups")
                ToolTip.visible: hovered
                ToolTip.text: root.t("title.manageGroups")
                onClicked: if (root.win && root.win.dialogs) root.win.dialogs.openGroups()
                background: Rectangle {
                    radius: root.r
                    color: groupsBtn.hovered ? root.fill : root.controlBg
                    border.width: 1
                    border.color: root.sep
                }
                contentItem: Text {
                    text: root.t("tb.groups")
                    color: root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 12
                    font.weight: Font.Medium
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }

            AbstractButton {
                id: refreshBtn
                height: 24
                padding: 0
                leftPadding: 8
                rightPadding: 8
                implicitWidth: Math.max(24, implicitContentWidth + leftPadding + rightPadding)
                hoverEnabled: true
                enabled: !(root.win && root.win.dialogs && root.win.dialogs.subUpdating)
                opacity: enabled ? 1 : 0.45
                Accessible.name: root.t("tb.refresh")
                ToolTip.visible: hovered
                ToolTip.text: root.t("title.refresh")
                onClicked: if (root.win && root.win.dialogs) root.win.dialogs.refreshSub()
                background: Rectangle {
                    radius: root.r
                    color: refreshBtn.hovered ? root.fill : root.controlBg
                    border.width: 1
                    border.color: root.sep
                }
                contentItem: Text {
                    text: root.t("tb.refresh")
                    color: root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 12
                    font.weight: Font.Medium
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
        }
    }

    Component {
        id: appSepComp
        Rectangle {
            width: parent ? parent.width : 212
            height: 9
            color: "transparent"
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.margins: 6
                height: 1
                color: root.sep
            }
        }
    }
    Component {
        id: appRowComp
        AbstractButton {
            id: appRow
            property var itemData: ({ k: "", act: "" })
            height: 28
            width: parent ? parent.width : 212
            hoverEnabled: true
            Accessible.name: root.t(appRow.itemData.k)
            Accessible.role: Accessible.MenuItem
            ToolTip.visible: hovered && !!appRow.itemData.tip
            ToolTip.text: appRow.itemData.tip ? root.t(appRow.itemData.tip) : ""
            onClicked: {
                appMenu.close()
                var d = root.win ? root.win.dialogs : null
                if (!d) return
                if (appRow.itemData.act === "add-clip") d.importClip()
                else if (appRow.itemData.act === "add-file") d.importFile()
                else if (appRow.itemData.act === "scan-qr") d.importQr()
                else if (appRow.itemData.act === "export") d.openExport()
                else if (appRow.itemData.act === "stats") d.openStats()
                else if (appRow.itemData.act === "hide") d.hideWin()
                else if (appRow.itemData.act === "quit") d.requestQuit()
            }
            background: Rectangle {
                radius: 4
                color: appRow.hovered ? root.fill : "transparent"
            }
            contentItem: Text {
                text: root.t(appRow.itemData.k)
                color: root.label
                font.family: root.fonts[0]
                font.pixelSize: 13
                verticalAlignment: Text.AlignVCenter
                leftPadding: 8
                rightPadding: 8
                elide: Text.ElideRight
            }
        }
    }
    Component {
        id: sepComp
        Rectangle {
            width: parent ? parent.width : 200
            height: 9
            color: "transparent"
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.margins: 6
                height: 1
                color: root.sep
            }
        }
    }
    Component {
        id: rowComp
        AbstractButton {
            id: row
            property var itemData: ({ k: "" })
            height: 28
            width: parent ? parent.width : 200
            hoverEnabled: true
            Accessible.name: root.t(row.itemData.k)
            ToolTip.visible: hovered && !!row.itemData.tip
            ToolTip.text: row.itemData.tip ? root.t(row.itemData.tip) : ""
            onClicked: {
                testMenu.close()
                var d = root.win ? root.win.dialogs : null
                if (!d) return
                if (row.itemData.k === "test.urlSelected") d.testRun("selected")
                else if (row.itemData.k === "test.urlGroup") d.testRun("group")
                else if (row.itemData.k === "test.stop") d.testStop()
                else if (row.itemData.k === "test.clear") d.testClear()
            }
            background: Rectangle {
                radius: 4
                color: row.hovered ? root.fill : "transparent"
            }
            contentItem: Text {
                text: root.t(row.itemData.k)
                color: root.label
                font.family: root.fonts[0]
                font.pixelSize: 13
                verticalAlignment: Text.AlignVCenter
                leftPadding: 8
            }
        }
    }
}
