pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T

T.ToolTip {
    id: tip

    property var hoverTarget: parent
    property bool active: hoverTarget && hoverTarget.hovered === true

    visible: active && text.length > 0
    delay: 400
    timeout: 8000
    padding: 8
    margins: 8
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent | T.Popup.CloseOnReleaseOutsideParent

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    x: parent ? Math.round((parent.width - implicitWidth) / 2) : 0
    y: parent ? -implicitHeight - 6 : 0

    contentItem: Item {
        implicitWidth: Math.min(Math.max(metrics.width, 1), 280)
        implicitHeight: body.implicitHeight

        Text {
            id: body
            width: parent.width
            text: tip.text
            color: tip.palette.toolTipText
            font.pixelSize: 12
            wrapMode: metrics.width > 280 ? Text.Wrap : Text.NoWrap
        }

        TextMetrics {
            id: metrics
            font: body.font
            text: tip.text
        }
    }

    background: Rectangle {
        radius: 6
        color: tip.palette.toolTipBase
        border.width: 1
        border.color: tip.palette.mid
    }
}
