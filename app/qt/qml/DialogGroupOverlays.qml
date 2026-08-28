import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: layer

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
    function editName() { return geName.text }
    function editUrl() { return geUrl.text }

    DialogMask {
        host: layer.host
        id: groupsMask
        DialogCard {
            host: layer.host
            cardW: 500
            height: Math.min(520, Math.max(260, groupsBody.implicitHeight + 32))
            ColumnLayout {
                id: groupsBody
                anchors.fill: parent
                anchors.margins: 16
                spacing: 8
                Text {
                    text: layer.host.t("groups.title")
                    color: layer.host.label
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    Layout.fillWidth: true
                }
                Text {
                    text: layer.host.t("groups.sub")
                    color: layer.host.secondary
                    wrapMode: Text.WordWrap
                    font.family: layer.host.fonts[0]
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
                        radius: layer.host.r
                        color: selected ? layer.host.fill : (rowHit.containsMouse ? layer.host.fill : "transparent")
                        border.width: 1
                        border.color: selected ? layer.host.blue : layer.host.menuBorder
                        activeFocusOnTab: true
                        Accessible.role: Accessible.ListItem
                        Accessible.name: groupRow.name + ", " + groupRow.count + " " + layer.host.t("js.nodes")
                        readonly property bool selected: {
                            var currentHome = layer.host.home
                            var cur = (currentHome && currentHome.activeGid) || layer.host.gid()
                            return cur === gid
                        }
                        function activate() {
                            if (layer.host.home && typeof layer.host.home.switchGroup === "function")
                                layer.host.home.switchGroup(groupRow.gid, true)
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
                                    color: layer.host.label
                                    font.family: layer.host.fonts[0]
                                    font.pixelSize: 13
                                    font.weight: groupRow.selected ? Font.DemiBold : Font.Medium
                                    elide: Text.ElideRight
                                }
                                Text {
                                    text: layer.host.t("js.nodes") + " · " + groupRow.count
                                    color: layer.host.secondary
                                    font.family: layer.host.fonts[0]
                                    font.pixelSize: 11
                                }
                            }
                            Row {
                                z: 1
                                Layout.alignment: Qt.AlignVCenter
                                spacing: 6
                                DialogButton { host: layer.host; uniform: true; text: layer.host.t("ctx.edit"); onClicked: layer.host.openGroupEdit(groupRow.gid, "edit") }
                                DialogButton { host: layer.host; uniform: true; text: layer.host.t("ctx.delete"); danger: true; onClicked: layer.host.deleteGroup(groupRow.gid) }
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    DialogButton { host: layer.host; uniform: true; text: layer.host.t("groups.new"); onClicked: layer.host.openGroupEdit(null, "create") }
                    Item { Layout.fillWidth: true }
                    DialogButton { host: layer.host; uniform: true;
                        text: layer.host.t("groups.updateAll")
                        enabled: !layer.host.subUpdating
                        onClicked: layer.host.updateAllGroups()
                    }
                    DialogButton { host: layer.host; uniform: true; text: layer.host.t("groups.done"); primary: true; onClicked: groupsMask.visible = false }
                }
            }
        }
    }

    DialogMask {
        host: layer.host
        id: groupEditMask
        z: 410
        DialogCard {
            host: layer.host
            cardW: 420
            implicitHeight: groupEditBody.implicitHeight + 32
            ColumnLayout {
                id: groupEditBody
                anchors.fill: parent
                anchors.margins: 16
                spacing: 8
                Text {
                    id: geTitle
                    color: layer.host.label
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    Layout.fillWidth: true
                }
                Text {
                    id: geSub
                    color: layer.host.secondary
                    wrapMode: Text.WordWrap
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                Item { Layout.preferredHeight: 2 }
                Text {
                    text: layer.host.t("edit.name")
                    color: layer.host.secondary
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                TextField {
                    id: geName
                    Layout.fillWidth: true
                    Layout.preferredHeight: 32
                    leftPadding: 10
                    rightPadding: 10
                    placeholderText: layer.host.t("edit.name")
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 13
                    color: layer.host.label
                    selectByMouse: true
                    Accessible.name: layer.host.t("edit.name")
                    Keys.onReturnPressed: layer.host.saveGroupEdit()
                    Keys.onEnterPressed: layer.host.saveGroupEdit()
                    onTextEdited: layer.host.groupEditError = ""
                    background: Rectangle {
                        radius: layer.host.r
                        color: layer.host.menuBg
                        border.width: 1
                        border.color: geName.activeFocus ? layer.host.blue : layer.host.menuBorder
                    }
                }
                Text {
                    text: layer.host.t("label.728c7d71")
                    color: layer.host.secondary
                    font.family: layer.host.fonts[0]
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
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 13
                    color: layer.host.label
                    selectByMouse: true
                    inputMethodHints: Qt.ImhUrlCharactersOnly
                    Accessible.name: layer.host.t("label.728c7d71")
                    onTextEdited: layer.host.groupEditError = ""
                    background: Rectangle {
                        radius: layer.host.r
                        color: layer.host.menuBg
                        border.width: 1
                        border.color: geUrl.activeFocus ? layer.host.blue : layer.host.menuBorder
                    }
                }
                Text {
                    visible: !!layer.host.groupEditError
                    text: layer.host.groupEditError
                    color: layer.host.red
                    wrapMode: Text.WordWrap
                    font.family: layer.host.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Item { Layout.fillWidth: true }
                    DialogButton { host: layer.host; uniform: true; text: layer.host.t("btn.cancel"); onClicked: groupEditMask.visible = false }
                    DialogButton { host: layer.host; uniform: true; text: layer.host.t("btn.save"); primary: true; onClicked: layer.host.saveGroupEdit() }
                }
            }
        }
    }

}
