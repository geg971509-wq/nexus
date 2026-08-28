pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var host
    width: 0
    height: 0

    ListModel { id: groupModel }

    function setRows(rows) {
        groupModel.clear()
        for (var i = 0; i < rows.length; i++) groupModel.append(rows[i])
    }
    function showGroups() { groupsMask.visible = true }
    function setEditForm(title, subtitle, name, url) {
        geTitle.text = title
        geSub.text = subtitle
        geName.text = name
        geUrl.text = url
    }
    function showEdit() { groupEditMask.visible = true; focusName() }
    function hideEdit() { groupEditMask.visible = false }
    function focusName() { geName.forceActiveFocus(); geName.selectAll() }
    function focusUrl() { geUrl.forceActiveFocus(); geUrl.selectAll() }
    function editName() { return geName.text }
    function editUrl() { return geUrl.text }

    DialogMask {
        host: root.host
        id: groupsMask
        DialogCard {
            host: root.host
            cardW: 500
            height: Math.min(520, Math.max(260, groupsBody.implicitHeight + 32))
            ColumnLayout {
                id: groupsBody
                anchors.fill: parent
                anchors.margins: 16
                spacing: 8
                Text {
                    text: root.host.t("groups.title")
                    color: root.host.label
                    font.family: root.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    Layout.fillWidth: true
                }
                Text {
                    text: root.host.t("groups.sub")
                    color: root.host.secondary
                    wrapMode: Text.WordWrap
                    font.family: root.host.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                ListView {
                    id: groupList
                    model: groupModel
                    clip: true
                    spacing: 4
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: 88
                    Layout.preferredHeight: Math.min(contentHeight, 316)
                    ScrollBar.vertical: ScrollBar { policy: groupList.contentHeight > groupList.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
                    delegate: Rectangle {
                        id: groupRow
                        required property string gid
                        required property string name
                        required property int count
                        width: groupList.width
                        height: 42
                        radius: root.host.r
                        color: selected ? root.host.fill : (rowHit.containsMouse ? root.host.fill : "transparent")
                        border.width: 1
                        border.color: selected ? root.host.blue : root.host.menuBorder
                        activeFocusOnTab: true
                        Accessible.role: Accessible.ListItem
                        Accessible.name: groupRow.name + ", " + groupRow.count + " " + root.host.t("js.nodes")
                        readonly property bool selected: {
                            var currentHome = root.host.home
                            var cur = (currentHome && currentHome.activeGid) || root.host.gid()
                            return cur === gid
                        }
                        function activate() {
                            if (root.host.home && typeof root.host.home.switchGroup === "function")
                                root.host.home.switchGroup(groupRow.gid, true)
                        }
                        Keys.onReturnPressed: activate()
                        Keys.onEnterPressed: activate()
                        Keys.onSpacePressed: activate()
                        MouseArea {
                            id: rowHit
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: groupRow.activate()
                        }
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 6
                            spacing: 8
                            Column {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignVCenter
                                spacing: 1
                                Text {
                                    width: parent.width
                                    text: groupRow.name
                                    color: root.host.label
                                    font.family: root.host.fonts[0]
                                    font.pixelSize: 13
                                    font.weight: groupRow.selected ? Font.DemiBold : Font.Medium
                                    elide: Text.ElideRight
                                }
                                Text {
                                    text: root.host.t("js.nodes") + " · " + groupRow.count
                                    color: root.host.secondary
                                    font.family: root.host.fonts[0]
                                    font.pixelSize: 11
                                }
                            }
                            Row {
                                z: 1
                                Layout.alignment: Qt.AlignVCenter
                                spacing: 6
                                DialogButton { host: root.host; uniform: true; text: root.host.t("ctx.edit"); onClicked: root.host.openGroupEdit(groupRow.gid, "edit") }
                                DialogButton { host: root.host; uniform: true; text: root.host.t("ctx.delete"); danger: true; onClicked: root.host.deleteGroup(groupRow.gid) }
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    DialogButton { host: root.host; uniform: true; text: root.host.t("groups.new"); onClicked: root.host.openGroupEdit(null, "create") }
                    Item { Layout.fillWidth: true }
                    DialogButton { host: root.host; uniform: true;
                        text: root.host.t("groups.updateAll")
                        enabled: !root.host.subUpdating
                        onClicked: root.host.updateAllGroups()
                    }
                    DialogButton { host: root.host; uniform: true; text: root.host.t("groups.done"); primary: true; onClicked: groupsMask.visible = false }
                }
            }
        }
    }

    DialogMask {
        host: root.host
        id: groupEditMask
        z: 410
        DialogCard {
            host: root.host
            cardW: 420
            implicitHeight: groupEditBody.implicitHeight + 32
            ColumnLayout {
                id: groupEditBody
                anchors.fill: parent
                anchors.margins: 16
                spacing: 8
                Text {
                    id: geTitle
                    color: root.host.label
                    font.family: root.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    Layout.fillWidth: true
                }
                Text {
                    id: geSub
                    color: root.host.secondary
                    wrapMode: Text.WordWrap
                    font.family: root.host.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                Item { Layout.preferredHeight: 2 }
                Text {
                    text: root.host.t("edit.name")
                    color: root.host.secondary
                    font.family: root.host.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                TextField {
                    id: geName
                    Layout.fillWidth: true
                    Layout.preferredHeight: 32
                    leftPadding: 10
                    rightPadding: 10
                    placeholderText: root.host.t("edit.name")
                    font.family: root.host.fonts[0]
                    font.pixelSize: 13
                    color: root.host.label
                    selectByMouse: true
                    Accessible.name: root.host.t("edit.name")
                    Keys.onReturnPressed: root.host.saveGroupEdit()
                    Keys.onEnterPressed: root.host.saveGroupEdit()
                    onTextEdited: root.host.groupEditError = ""
                    background: Rectangle {
                        radius: root.host.r
                        color: root.host.menuBg
                        border.width: 1
                        border.color: geName.activeFocus ? root.host.blue : root.host.menuBorder
                    }
                }
                Text {
                    text: root.host.t("label.728c7d71")
                    color: root.host.secondary
                    font.family: root.host.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                TextField {
                    id: geUrl
                    Layout.fillWidth: true
                    Layout.preferredHeight: 32
                    leftPadding: 10
                    rightPadding: 10
                    placeholderText: "https://…/sub"
                    font.family: root.host.fonts[0]
                    font.pixelSize: 13
                    color: root.host.label
                    selectByMouse: true
                    inputMethodHints: Qt.ImhUrlCharactersOnly
                    Accessible.name: root.host.t("label.728c7d71")
                    onTextEdited: root.host.groupEditError = ""
                    background: Rectangle {
                        radius: root.host.r
                        color: root.host.menuBg
                        border.width: 1
                        border.color: geUrl.activeFocus ? root.host.blue : root.host.menuBorder
                    }
                }
                Text {
                    visible: !!root.host.groupEditError
                    text: root.host.groupEditError
                    color: root.host.red
                    wrapMode: Text.WordWrap
                    font.family: root.host.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Item { Layout.fillWidth: true }
                    DialogButton { host: root.host; uniform: true; text: root.host.t("btn.cancel"); onClicked: groupEditMask.visible = false }
                    DialogButton { host: root.host; uniform: true; text: root.host.t("btn.save"); primary: true; onClicked: root.host.saveGroupEdit() }
                }
            }
        }
    }

}
