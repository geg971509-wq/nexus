pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window

Item {
    id: root
    height: th ? th.statusH : 28
    implicitHeight: height

    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
    readonly property var mono: th ? th.monoFamilies : ["Menlo"]
    readonly property color chrome: th ? th.chromeSolid : "#fafafc"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color tertiary: th ? th.tertiary : "#8e8e93"
    readonly property color secondary: th ? th.secondary : "#6e6e73"

    function t(k) { return i18 ? i18.t(k) : k }

    Rectangle {
        anchors.fill: parent
        color: root.chrome
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 1
        color: root.sep
    }

    Item {
        anchors.fill: parent

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            text: root.win ? root.win.sbStatus : root.t("sb.stopped")
            color: root.tertiary
            font.family: root.fonts[0]
            font.pixelSize: 11
            Accessible.name: text
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.verticalCenter: parent.verticalCenter
            text: root.t("sb.mixed") + " · " + (root.win ? root.win.mixedListen : "127.0.0.1:2080")
            color: root.secondary
            font.family: root.mono[0]
            font.pixelSize: 11
            font.weight: Font.Medium
            Accessible.name: root.t("title.sbMixed")
            ToolTip.visible: mixedHover.containsMouse
            ToolTip.text: root.t("title.sbMixed")
            MouseArea {
                id: mixedHover
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
            }
        }

        Row {
            anchors.right: parent.right
            anchors.rightMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            spacing: 14
            Text {
                text: root.t("sb.proxy") + "  " + (root.win ? root.win.sbProxy : "—")
                color: root.tertiary
                font.family: root.fonts[0]
                font.pixelSize: 11
                ToolTip.visible: pHover.containsMouse
                ToolTip.text: root.t("title.sbProxy")
                MouseArea { id: pHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
            }
            Text {
                text: root.t("sb.direct") + "  " + (root.win ? root.win.sbDirect : "—")
                color: root.tertiary
                font.family: root.fonts[0]
                font.pixelSize: 11
                ToolTip.visible: dHover.containsMouse
                ToolTip.text: root.t("title.sbDirect")
                MouseArea { id: dHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
            }
        }
    }
}
