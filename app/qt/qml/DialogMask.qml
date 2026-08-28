import QtQuick
import QtQuick.Controls

Rectangle {
    id: mask

    required property var host
    property var dismiss: function () { mask.visible = false }

    parent: Overlay.overlay
    anchors.fill: parent
    z: 400
    color: host.scrim
    visible: false
    enabled: visible

    MouseArea {
        anchors.fill: parent
        enabled: mask.visible
        onClicked: mask.dismiss()
    }
}
