pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

AbstractButton {
    id: button

    required property var host
    property bool primary: false
    property bool danger: false
    property bool uniform: false

    height: 30
    implicitWidth: uniform ? 112 : Math.max(72, label.implicitWidth + 32)
    hoverEnabled: true
    opacity: enabled ? 1 : 0.45
    focusPolicy: Qt.StrongFocus
    Accessible.name: text

    background: Rectangle {
        radius: button.uniform ? host.r : 8
        color: button.danger
            ? (button.hovered || button.down ? Qt.darker(host.red, 1.08) : host.red)
            : (button.primary
               ? (button.hovered || button.down ? Qt.darker(host.blue, 1.08) : host.blue)
               : (button.hovered || button.down ? host.fill : host.menuBg))
        border.width: button.uniform ? 1 : 0
        border.color: button.activeFocus ? host.blue : host.menuBorder
    }

    contentItem: Text {
        id: label
        text: button.text
        color: button.primary || button.danger ? "#ffffff" : host.label
        font.family: host.fonts[0]
        font.pixelSize: button.uniform ? 12 : 13
        font.weight: button.uniform ? Font.Medium
                                    : (button.primary || button.danger ? Font.DemiBold : Font.Medium)
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
}
