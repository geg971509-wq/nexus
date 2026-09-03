pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

Item {
    id: root
    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
    readonly property var mono: th ? th.monoFamilies : ["Menlo"]
    readonly property color bg: th ? th.bg : "#f5f5f7"
    readonly property color label: th ? th.label : "#1d1d1f"
    readonly property color secondary: th ? th.secondary : "#6e6e73"
    readonly property color tertiary: th ? th.tertiary : "#8e8e93"
    readonly property color quaternary: th ? th.quaternary : "#aeaeb2"
    readonly property color blue: th ? th.blue : "#007aff"
    readonly property color blueSoft: th ? th.blueSoft : "#1a007aff"
    readonly property color green: th ? th.green : "#34c759"
    readonly property color greenSoft: th ? th.greenSoft : "#2434c759"
    readonly property color surface: th ? th.surface : "#ffffff"
    readonly property color heroTop: th ? th.heroTop : "#ffffff"
    readonly property color heroBot: th ? th.heroBot : "#fbfbfd"
    readonly property color heroBorder: th ? th.heroBorder : "#0b000000"
    readonly property color tableBorder: th ? th.tableBorder : "#0b000000"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color chrome: th ? th.chromeSolid : "#fafafc"
    readonly property color fill: th ? th.fill : "#1e787880"
    readonly property color fill2: th ? th.fill2 : "#14787880"
    readonly property color knob: th ? th.knob : "#ffffff"
    readonly property color switchTrack: th ? th.switchTrack : "#51787880"


    HomeController { id: flow; host: root }

    property alias connected: flow.connected
    property alias powerBusy: flow.powerBusy
    property alias powerOp: flow.powerOp
    property alias powerError: flow.powerError
    property alias tunOn: flow.tunOn
    property alias tunBusy: flow.tunBusy
    property alias tunWant: flow.tunWant
    property alias sysOn: flow.sysOn
    property alias sysBusy: flow.sysBusy
    property alias sysWant: flow.sysWant
    property alias selectedName: flow.selectedName
    property alias selectedLat: flow.selectedLat
    property alias connectedName: flow.connectedName
    property alias connectedLat: flow.connectedLat
    property alias activeGid: flow.activeGid
    property alias catalog: flow.catalog
    property alias applyingChip: flow.applyingChip
    property alias coreBaseUp: flow.coreBaseUp
    property alias coreBaseDown: flow.coreBaseDown
    property alias connPollFailStreak: flow.connPollFailStreak
    property alias connectedAt: flow.connectedAt

    property alias table: table
    property alias dockControl: dock
    property alias tunControl: tunChip
    property alias sysControl: sysChip
    property alias statusLabelControl: statusLabel
    property alias statusSubControl: statusSub

    function apiObject() {
        // nexus is an intentionally injected C++ context property.
        // qmllint disable unqualified
        if (typeof nexus === "undefined" || !nexus) return null
        if (typeof nexus.invoke !== "function") return null
        return nexus
        // qmllint enable unqualified
    }

    function t(k, v) { return flow.t(k, v) }
    function api() { return flow.api() }
    function parseReply(raw) { return flow.parseReply(raw) }
    function invoke(cmd, payload) { return flow.invoke(cmd, payload) }
    function log(tag, cls, msg) { flow.log(tag, cls, msg) }
    function connectPayload(name) { return flow.connectPayload(name) }
    function heroStatus() { flow.heroStatus() }
    function loadCatalog() { flow.loadCatalog() }
    function switchGroup(id, logIt) { flow.switchGroup(id, logIt) }
    function startNamed(name) { flow.startNamed(name) }
    function stopTunnel() { flow.stopTunnel() }
    function applyTun(on) { flow.applyTun(on) }
    function applySys(on) { flow.applySys(on) }
    function togglePower() { flow.togglePower() }
    function syncConnPoll() { flow.syncConnPoll() }
    function refreshConns() { flow.refreshConns() }
    function refreshSbProxy() { flow.refreshSbProxy() }

    Component.onCompleted: flow.initialize()

    Rectangle { anchors.fill: parent; color: root.bg }

    Column {
        anchors.fill: parent
        spacing: 0

        Item {
            id: hero
            width: parent.width
            height: 112
            Rectangle {
                id: card
                anchors.fill: parent
                radius: 0
                border.width: 0
                gradient: Gradient {
                    GradientStop { position: 0; color: root.heroTop }
                    GradientStop { position: 1; color: root.heroBot }
                }

                Titlebar {
                    id: heroTools
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                }

                RowLayout {
                    id: bar
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: heroTools.bottom
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    spacing: 8

                    Item {
                        id: powerBox
                        opacity: powerHit.enabled ? 1 : 0.45
                        implicitWidth: 32
                        implicitHeight: 32
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 32
                        Layout.minimumWidth: 32
                        Layout.maximumWidth: 32
                        Layout.alignment: Qt.AlignVCenter
                        Rectangle {
                            anchors.fill: parent
                            radius: 6
                            color: root.connected ? root.green : (powerHit.containsMouse ? root.fill : root.fill2)
                            border.width: powerHit.activeFocus ? 2 : 1
                            border.color: powerHit.activeFocus ? root.blue : (root.connected ? root.green : root.sep)
                        }
                        Text {
                            anchors.fill: parent
                            text: "⏻"
                            color: root.connected ? "#ffffff" : root.secondary
                            font.pixelSize: 16
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                    }

                    Column {
                        Layout.alignment: Qt.AlignVCenter
                        Layout.fillWidth: true
                        Layout.minimumWidth: 80
                        spacing: 1
                        Row {
                            spacing: 6
                            Rectangle {
                                width: 7
                                height: 7
                                radius: 4
                                color: root.connected ? root.green : root.quaternary
                                border.width: root.connected ? 3 : 0
                                border.color: root.greenSoft
                                anchors.verticalCenter: parent.verticalCenter
                            }
                            Text {
                                id: statusLabel
                                text: root.t("status.disconnected")
                                color: root.label
                                font.family: root.fonts[0]
                                font.pixelSize: 13
                                font.weight: Font.DemiBold
                            }
                        }
                        Text {
                            id: statusSub
                            text: root.t("status.subOff", { name: "—" })
                            color: root.secondary
                            font.family: root.fonts[0]
                            font.pixelSize: 12
                            elide: Text.ElideRight
                            width: Math.min(implicitWidth, parent.width)
                        }
                    }

                    Row {
                        id: chips
                        Layout.alignment: Qt.AlignVCenter
                        spacing: 6
                        Chip {
                            id: tunChip
                            text: root.t("sec.6f9bbe3d")
                            checked: false
                            enabled: !root.powerBusy && !root.tunBusy && !root.sysBusy
                            tooltip: root.t("title.tun")
                            onClicked: root.applyTun(checked)
                        }
                        Chip {
                            id: sysChip
                            text: root.t("sec.90c0bb9a")
                            checked: true
                            enabled: !root.powerBusy && !root.tunBusy && !root.sysBusy
                            tooltip: root.t("title.sysProxy")
                            onClicked: root.applySys(checked)
                        }
                    }
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 1
                    color: root.heroBorder
                }

                MouseArea {
                    id: powerHit
                    enabled: !root.powerBusy && !root.tunBusy && !root.sysBusy
                    activeFocusOnTab: true
                    width: 44
                    height: 44
                    x: 10 + (32 - width) / 2
                    y: heroTools.height + (parent.height - heroTools.height - height) / 2
                    z: 100
                    preventStealing: true
                    acceptedButtons: Qt.LeftButton
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    Accessible.role: Accessible.Button
                    Accessible.name: root.t("title.power")
                    Accessible.checkable: true
                    Accessible.checked: root.connected
                    Accessible.onPressAction: root.togglePower()
                    Tip { active: powerHit.containsMouse; text: root.t("title.power") }
                    Keys.onPressed: function (event) {
                        if (event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            event.accepted = true
                            root.togglePower()
                        }
                    }
                    onPressed: {
                        forceActiveFocus()
                        root.togglePower()
                    }
                }
            }
        }

        NodeTable {
            id: table
            width: parent.width
            height: parent.height - hero.height - dock.height
            connected: root.connected
            connectedName: root.connectedName
            onNodeChosen: function (name, lat) {
                root.selectedName = name
                root.selectedLat = lat
                root.heroStatus()
            }
            onNodeEdit: function (name) {
                if (win && win.dialogs) win.dialogs.openEdit(name)
            }
            onNodeContext: function (gx, gy) {
                if (win && win.dialogs) win.dialogs.openNodeCtx(gx, gy)
            }
            onSelectAllDone: function (n) {
                root.log("SYS", "info", root.t("log.selectedN", { n: n }))
            }
        }

        Dock {
            id: dock
            width: parent.width
            onOpenChanged: { root.syncConnPoll(); if (open && panel === "conn") root.refreshConns() }
            onPanelChanged: { root.syncConnPoll(); if (open && panel === "conn") root.refreshConns() }
        }
    }

    component Chip: AbstractButton {
        id: chip
        property string tooltip: ""
        opacity: enabled ? 1 : 0.45
        checkable: true
        height: 28
        hoverEnabled: true
        Accessible.name: text
        Accessible.checkable: true
        Accessible.checked: checked
        Tip { text: chip.tooltip }
        background: Rectangle {
            radius: 999
            color: chip.checked ? root.blueSoft : (chip.hovered ? root.fill : root.fill2)
        }
        contentItem: Row {
            spacing: 7
            leftPadding: 8
            rightPadding: 11
            Item {
                width: 28
                height: 16
                anchors.verticalCenter: parent.verticalCenter
                Rectangle {
                    anchors.fill: parent
                    radius: 999
                    color: chip.checked ? root.blue : root.switchTrack
                }
                Rectangle {
                    width: 13
                    height: 13
                    radius: 7
                    color: root.knob
                    y: 1.5
                    x: chip.checked ? 13.5 : 1.5
                    Behavior on x { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
                }
            }
            Text {
                text: chip.text
                color: chip.checked ? root.blue : root.secondary
                font.family: root.fonts[0]
                font.pixelSize: 12
                font.weight: Font.Medium
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        implicitWidth: contentItem.implicitWidth
        implicitHeight: 28
    }
}
