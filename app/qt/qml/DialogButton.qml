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
        radius: button.uniform ? button.host.r : 8
        color: button.danger
            ? (button.hovered || button.down ? Qt.darker(button.host.red, 1.08) : button.host.red)
            : (button.primary
               ? (button.hovered || button.down ? Qt.darker(button.host.blue, 1.08) : button.host.blue)
               : (button.hovered || button.down ? button.host.fill : button.host.menuBg))
        border.width: button.uniform ? 1 : 0
        border.color: button.activeFocus ? button.host.blue : button.host.menuBorder
    }

    contentItem: Text {
        id: label
        text: button.text
        color: button.primary || button.danger ? "#ffffff" : button.host.label
        font.family: button.host.fonts[0]
        font.pixelSize: button.uniform ? 12 : 13
        font.weight: button.uniform ? Font.Medium
                                    : (button.primary || button.danger ? Font.DemiBold : Font.Medium)
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
}
