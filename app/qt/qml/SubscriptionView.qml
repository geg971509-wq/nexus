pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window

Item {
    id: root

    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
    readonly property color bg: th ? th.bg : "#f5f5f7"
    readonly property color label: th ? th.label : "#1d1d1f"
    readonly property color tertiary: th ? th.tertiary : "#8e8e93"
    readonly property color blue: th ? th.blue : "#007aff"
    readonly property color red: th ? th.red : "#ff3b30"
    readonly property color surface: th ? th.surface : "#ffffff"
    readonly property color chrome: th ? th.chromeSolid : "#fafafc"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color hairline: th ? th.hairline : "#0b000000"
    readonly property color fill: th ? th.fill : "#1e787880"
    readonly property color controlBg: th ? th.controlBg : "#ffffff"
    readonly property color controlText: th ? th.controlText : "#000000"
    readonly property color rowHover: th ? th.sideHover : "#09000000"
    readonly property bool dark: th ? th.dark : false
    readonly property int rLg: th ? th.radiusLg : 14
    readonly property color sectionLabel: dark ? Qt.rgba(0.922, 0.922, 0.961, 0.48)
                                               : Qt.rgba(0.235, 0.235, 0.263, 0.58)
    readonly property color controlBorder: dark ? Qt.rgba(1, 1, 1, 0.12) : Qt.rgba(0, 0, 0, 0.14)
    readonly property color controlBorderHover: dark ? Qt.rgba(1, 1, 1, 0.26) : Qt.rgba(0, 0, 0, 0.28)
    readonly property bool subBusy: !!(win && win.dialogs && win.dialogs.subUpdating)
    readonly property string lang: i18 ? i18.lang : "zh-CN"

    property string subUrlError: ""
    property string groupName: "Default"
    property var catalog: null

    BackendClient {
        id: backend
        bridge: root.api()
    }

    function t(k, v) {
        var _ = lang
        return i18 ? i18.t(k, v) : k
    }

    function api() {
        // nexus is an intentionally injected C++ context property.
        // qmllint disable unqualified
        return (typeof nexus === "undefined") ? null : nexus
        // qmllint enable unqualified
    }

    function invoke(cmd, payload) { return backend.invoke(cmd, payload) }

    function activeGroup() {
        var data = catalog
        if (!data || !data.groups || !data.groups.length)
            return { id: "default", name: "Default", url: "" }
        var gid = data.active
        if (win && win.home && win.home.activeGid)
            gid = win.home.activeGid
        for (var i = 0; i < data.groups.length; i++) {
            if (data.groups[i].id === gid)
                return data.groups[i]
        }
        return data.groups[0]
    }

    function loadCatalog() {
        var r = invoke("catalog_get", {})
        catalog = backend.unwrapCatalog(r && r.ok ? (r.data || r) : (r && r.data))
        var g = activeGroup()
        groupName = g.name || "Default"
        urlField.text = g.url || ""
        subUrlError = ""
    }

    function applySub() {
        if (subBusy) return
        var url = (urlField.text || "").trim()
        subUrlError = ""
        if (url && (!/^https?:\/\/\S+$/i.test(url) || url.length > 4096)) {
            subUrlError = t("error.subUrlHttp")
            urlField.forceActiveFocus()
            urlField.selectAll()
            return
        }
        var r = invoke("catalog_get", {})
        var data = backend.unwrapCatalog(r && r.ok ? (r.data || r) : (r && r.data))
        if (!data || !data.groups || !data.groups.length) {
            catalog = data
            return
        }
        catalog = data
        var g = activeGroup()
        if (!g) return
        g.url = url
        groupName = g.name || "Default"
        var saved = invoke("catalog_put", { blob: data })
        if (!saved || saved.ok === false || saved.offline) {
            subUrlError = t("error.catalogSave")
            return
        }
        if (url && win && win.dialogs && typeof win.dialogs.refreshSub === "function")
            win.dialogs.refreshSub()
    }

    onVisibleChanged: if (visible) loadCatalog()
    Component.onCompleted: loadCatalog()

    Rectangle { anchors.fill: parent; color: root.bg }

    Column {
        anchors.fill: parent
        spacing: 0

        Item {
            id: head
            width: parent.width
            height: 44
            Rectangle { anchors.fill: parent; color: root.chrome }
            Text {
                anchors.left: parent.left
                anchors.leftMargin: 24
                anchors.verticalCenter: parent.verticalCenter
                text: root.t("panel.sub")
                color: root.label
                font.family: root.fonts[0]
                font.pixelSize: 15
                font.weight: Font.DemiBold
            }
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 1
                color: root.sep
            }
        }

        Flickable {
            id: flick
            width: parent.width
            height: parent.height - head.height
            clip: true
            contentWidth: width
            contentHeight: rail.height + 40
            boundsBehavior: Flickable.StopAtBounds

            Item {
                id: rail
                width: Math.min(680, flick.width - 48)
                x: Math.round((flick.width - width) / 2)
                y: 20
                height: card.height

                Rectangle {
                    id: card
                    width: parent.width
                    height: subColumn.height
                    radius: root.rLg
                    color: root.surface
                    border.width: 1
                    border.color: root.hairline

                    Column {
                        id: subColumn
                        width: parent.width

                        Text {
                            width: parent.width
                            height: 32
                            text: root.t("sec.0c7d604b")
                            color: root.sectionLabel
                            font.family: root.fonts[0]
                            font.pixelSize: 11
                            font.weight: Font.Medium
                            leftPadding: 16
                            topPadding: 10
                            verticalAlignment: Text.AlignVCenter
                        }

                        Item {
                            id: subRow
                            width: parent.width
                            height: Math.max(68, rowContent.implicitHeight + 24)

                            Rectangle {
                                anchors.fill: parent
                                color: rowHover.hovered ? root.rowHover : "transparent"
                                radius: root.rLg
                                Rectangle {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    height: parent.radius
                                    color: parent.color
                                }
                            }
                            HoverHandler { id: rowHover }
                            Rectangle {
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.top: parent.top
                                height: 1
                                color: root.sep
                            }
                            Item {
                                id: rowContent
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.leftMargin: 16
                                anchors.rightMargin: 16
                                implicitHeight: Math.max(rowLabel.implicitHeight, editorColumn.implicitHeight)
                                height: implicitHeight

                                Text {
                                    id: rowLabel
                                    anchors.left: parent.left
                                    anchors.top: parent.top
                                    anchors.topMargin: Math.max(0, (editorColumn.implicitHeight - implicitHeight) / 2)
                                    width: 140
                                    text: root.t("label.728c7d71")
                                    color: root.label
                                    font.family: root.fonts[0]
                                    font.pixelSize: 13
                                    font.weight: Font.Medium
                                    wrapMode: Text.Wrap
                                }

                                Column {
                                    id: editorColumn
                                    anchors.left: rowLabel.right
                                    anchors.leftMargin: 16
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    spacing: 5

                                    Text {
                                        width: parent.width
                                        text: root.subUrlError || root.t("hint.subUrlNamed", { name: root.groupName })
                                        color: root.subUrlError ? root.red : root.tertiary
                                        font.family: root.fonts[0]
                                        font.pixelSize: 11
                                        wrapMode: Text.Wrap
                                        lineHeight: 1.35
                                        lineHeightMode: Text.ProportionalHeight
                                    }

                                    Row {
                                        width: parent.width
                                        spacing: 8

                                        TextField {
                                            id: urlField
                                            width: parent.width - applyButton.width - 8
                                            height: 32
                                            placeholderText: "https://…/sub"
                                            color: root.controlText
                                            placeholderTextColor: root.tertiary
                                            font.family: root.fonts[0]
                                            font.pixelSize: 13
                                            font.weight: Font.Medium
                                            selectByMouse: true
                                            inputMethodHints: Qt.ImhUrlCharactersOnly
                                            clip: true
                                            leftPadding: 11
                                            rightPadding: 11
                                            verticalAlignment: Text.AlignVCenter
                                            onTextEdited: root.subUrlError = ""
                                            Keys.onReturnPressed: root.applySub()
                                            Keys.onEnterPressed: root.applySub()
                                            Accessible.name: root.t("label.728c7d71")
                                            background: Rectangle {
                                                radius: 8
                                                color: root.controlBg
                                                border.width: 1
                                                border.color: root.subUrlError ? root.red
                                                    : (urlField.activeFocus || urlHover.hovered
                                                       ? root.controlBorderHover : root.controlBorder)
                                                HoverHandler { id: urlHover }
                                            }
                                        }

                                        AbstractButton {
                                            id: applyButton
                                            height: 32
                                            implicitWidth: Math.max(52, applyText.implicitWidth + 22)
                                            text: root.t("btn.apply")
                                            enabled: !root.subBusy
                                            hoverEnabled: true
                                            opacity: enabled ? 1 : 0.45
                                            Accessible.name: root.t("title.applySub")
                                            onClicked: root.applySub()
                                            background: Rectangle {
                                                radius: 8
                                                color: applyButton.hovered && applyButton.enabled ? "#2e787880" : root.fill
                                            }
                                            contentItem: Text {
                                                id: applyText
                                                text: applyButton.text
                                                color: root.blue
                                                font.family: root.fonts[0]
                                                font.pixelSize: 12
                                                font.weight: Font.DemiBold
                                                horizontalAlignment: Text.AlignHCenter
                                                verticalAlignment: Text.AlignVCenter
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
