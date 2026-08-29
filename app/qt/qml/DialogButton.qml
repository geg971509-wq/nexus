pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

AbstractButton {
    id: button

    required property var host
    property bool primary: false
    property bool danger: false
    // Equal width is a layout concern; it must not create a second visual style.
    property bool uniform: false

    readonly property int controlHeight: 30
    readonly property int uniformWidth: 112

    // Plain Rows honor height, while Qt Quick Layouts size children from their
    // implicit/Layout metrics. Keep both paths pinned to the same dimensions.
    height: controlHeight
    implicitHeight: controlHeight
    implicitWidth: uniform ? uniformWidth : Math.max(72, label.implicitWidth + 32)

    Layout.minimumWidth: implicitWidth
    Layout.preferredWidth: implicitWidth
    Layout.maximumWidth: implicitWidth
    Layout.minimumHeight: implicitHeight
    Layout.preferredHeight: implicitHeight
    Layout.maximumHeight: implicitHeight

    hoverEnabled: true
    opacity: enabled ? 1 : 0.45
    focusPolicy: Qt.StrongFocus
    Accessible.name: text

    background: Rectangle {
        radius: button.host.rLg
        color: button.danger
            ? (button.hovered || button.down ? Qt.darker(button.host.red, 1.08) : button.host.red)
            : (button.primary
               ? (button.hovered || button.down ? Qt.darker(button.host.blue, 1.08) : button.host.blue)
               : (button.hovered || button.down ? button.host.fill : button.host.menuBg))
        border.width: button.activeFocus ? 1 : 0
        border.color: button.activeFocus ? button.host.blue : button.host.menuBorder
    }

    contentItem: Text {
        id: label
        text: button.text
        color: button.primary || button.danger ? "#ffffff" : button.host.label
        font.family: button.host.fonts[0]
        font.pixelSize: 13
        font.weight: button.primary || button.danger ? Font.DemiBold : Font.Medium
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
}
