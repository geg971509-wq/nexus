import QtQuick

Rectangle {
    required property var host
    property int cardW: 360

    width: Math.min(cardW, parent.width - 40)
    radius: host.rLg
    color: host.menuBg
    border.width: 1
    border.color: host.menuBorder
    anchors.centerIn: parent

    MouseArea { anchors.fill: parent }
}
