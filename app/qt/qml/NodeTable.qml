import QtQuick
import QtQuick.Controls
import QtQuick.Window

Item {
    id: root
    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
    readonly property var mono: th ? th.monoFamilies : ["Menlo"]
    readonly property bool dark: th ? th.dark : false
    readonly property color label: th ? th.label : "#1d1d1f"
    readonly property color secondary: th ? th.secondary : "#6e6e73"
    readonly property color tertiary: th ? th.tertiary : "#8e8e93"
    readonly property color quaternary: th ? th.quaternary : "#aeaeb2"
    readonly property color blue: th ? th.blue : "#007aff"
    readonly property color blueSoft: th ? th.blueSoft : "#1e007aff"
    readonly property color green: th ? th.green : "#34c759"
    readonly property color surface: th ? th.surface : "#ffffff"
    readonly property color tableBorder: th ? th.tableBorder : "#0b000000"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color latGood: th ? th.latGood : "#248a3d"
    readonly property color latMid: th ? th.latMid : "#c93400"
    readonly property color latBad: th ? th.latBad : "#d70015"
    readonly property color rowHover: dark ? "#0affffff" : "#05000000"
    readonly property color rowSelected: dark ? "#380a84ff" : "#1a007aff"
    readonly property color rowSelectedHover: dark ? "#470a84ff" : "#24007aff"
    readonly property color rowConnected: dark ? "#2930d158" : "#1f34c759"
    readonly property color rowConnectedHover: dark ? "#3d30d158" : "#2e34c759"
    readonly property color rowConnectedSel: dark ? "#3830d158" : "#2934c759"
    readonly property int idxW: 40
    readonly property int typeW: 84
    readonly property int addrW: 170
    readonly property int latW: 88
    readonly property int flowW: 180
    readonly property int nameW: Math.max(80, card.width - idxW - typeW - addrW - latW - flowW)

    property string selectedName: "—"
    property string selectedLat: "—"
    property var selectedNames: []
    property int selectAnchor: -1
    property string connectedName: ""
    property bool connected: false
    property string sortKey: "lat"
    property int sortDir: 1
    property var raw: []

    signal nodeChosen(string name, string lat)
    signal nodeEdit(string name)
    signal nodeContext(real gx, real gy)
    signal selectAllDone(int n)

    function t(k, v) { return i18 ? i18.t(k, v) : k }

    function fmtBytes(n) {
        n = Number(n) || 0
        if (n < 1024) return n + " B"
        if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB"
        if (n < 1024 * 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + " MB"
        return (n / (1024 * 1024 * 1024)).toFixed(2) + " GB"
    }

    function latKey(n) {
        var rawLat = n && n.lat
        if (rawLat == null || rawLat === "" || rawLat === "—" || rawLat === "…")
            return { known: false, ms: 0 }
        if (typeof rawLat === "number") {
            if (!isFinite(rawLat) || rawLat < 0) return { known: false, ms: 0 }
            return { known: true, ms: rawLat }
        }
        var s = String(rawLat)
        if (/timeout|fail|error|不可用|aborted/i.test(s)) return { known: false, ms: 0 }
        var m = s.match(/(-?\d+(?:\.\d+)?)/)
        if (!m) return { known: false, ms: 0 }
        var v = parseFloat(m[1])
        if (!isFinite(v) || v < 0) return { known: false, ms: 0 }
        return { known: true, ms: v }
    }

    function latText(n) {
        if (!n || n.lat == null || n.lat === "") return "—"
        if (typeof n.lat === "number") {
            if (!isFinite(n.lat)) return "—"
            if (n.lat < 0) return "timeout"
            return n.lat + " ms"
        }
        return String(n.lat)
    }

    function latKind(n) {
        var txt = latText(n)
        if (txt === "timeout") return "bad"
        var k = latKey(n)
        if (!k.known) return ""
        if (k.ms < 175) return "good"
        if (k.ms < 220) return "mid"
        return "bad"
    }

    function latColor(kind) {
        if (kind === "good") return root.latGood
        if (kind === "mid") return root.latMid
        if (kind === "bad") return root.latBad
        return root.quaternary
    }

    function flowText(n) {
        if (!n) return "—"
        if (n.flow && String(n.flow).length && String(n.flow).indexOf("—") < 0)
            return String(n.flow)
        var up = Math.max(0, Number(n.flowUp) || 0)
        var down = Math.max(0, Number(n.flowDown) || 0)
        if (!up && !down) return "—"
        return fmtBytes(up) + "↑ · " + fmtBytes(down) + "↓"
    }

    function flowBytes(n) {
        var up = Math.max(0, Number(n.flowUp) || 0)
        var down = Math.max(0, Number(n.flowDown) || 0)
        if (up || down) return up + down
        var t = flowText(n)
        if (!t || t.indexOf("—") >= 0) return -1
        var total = 0
        var re = /([\d.]+)\s*([KMG]?B)/gi
        var m
        while ((m = re.exec(t))) {
            var v = parseFloat(m[1])
            var u = m[2].toUpperCase()
            if (u.indexOf("K") === 0) v *= 1024
            else if (u.indexOf("M") === 0) v *= 1024 * 1024
            else if (u.indexOf("G") === 0) v *= 1024 * 1024 * 1024
            total += v
        }
        return total
    }

    function nodeByName(name) {
        var list = root.raw || []
        for (var i = 0; i < list.length; i++)
            if (list[i] && list[i].name === name) return list[i]
        return null
    }

    function firstName() {
        if (rows.count > 0) return rows.get(0).name
        var list = root.raw || []
        return (list[0] && list[0].name) ? list[0].name : ""
    }

    function setNodes(arr, keepName) {
        var copy = []
        var src = Array.isArray(arr) ? arr : []
        for (var i = 0; i < src.length; i++) {
            var n = src[i] || {}
            copy.push({
                origIdx: i,
                name: n.name || "",
                type: n.type || "",
                addr: n.addr || "",
                lat: (n.lat == null || n.lat === "") ? null : n.lat,
                flow: n.flow == null ? null : n.flow,
                flowUp: Math.max(0, Number(n.flowUp) || 0),
                flowDown: Math.max(0, Number(n.flowDown) || 0),
                link: n.link || "",
                outbound: n.outbound || null
            })
        }
        root.raw = copy
        var keep = keepName && copy.some(function (n) { return n.name === keepName })
        if (keep) {
            selectedName = keepName
            selectedLat = latText(nodeByName(keepName))
            selectedNames = [keepName]
        } else if (copy.length) {
            selectedName = copy[0].name
            selectedLat = latText(copy[0])
            selectedNames = [copy[0].name]
        } else {
            selectedName = "—"
            selectedLat = "—"
            selectedNames = []
        }
        selectAnchor = -1
        applySort()
        nodeChosen(selectedName, selectedLat)
    }

    function clickSort(key) {
        if (sortKey === key) sortDir = -sortDir
        else {
            sortKey = key
            sortDir = (key === "flow") ? -1 : 1
        }
        applySort()
    }

    function applySort() {
        var list = (root.raw || []).slice()
        var key = sortKey
        var dir = sortDir
        list.sort(function (a, b) {
            var cmp = 0
            if (key === "lat") {
                var ka = latKey(a)
                var kb = latKey(b)
                if (ka.known !== kb.known) cmp = ka.known ? -1 : 1
                else if (ka.known) cmp = (ka.ms - kb.ms) * dir
            } else if (key === "idx") {
                cmp = ((a.origIdx || 0) - (b.origIdx || 0)) * dir
            } else if (key === "flow") {
                cmp = (flowBytes(a) - flowBytes(b)) * dir
            } else if (key === "type") {
                cmp = String(a.type || "").localeCompare(String(b.type || ""), undefined, { sensitivity: "base" }) * dir
            } else if (key === "addr") {
                cmp = String(a.addr || "").localeCompare(String(b.addr || ""), undefined, { numeric: true, sensitivity: "base" }) * dir
            } else {
                cmp = String(a.name || "").localeCompare(String(b.name || ""), undefined, { numeric: true, sensitivity: "base" }) * dir
            }
            if (cmp === 0)
                cmp = String(a.name || "").localeCompare(String(b.name || ""))
            return cmp
        })
        rows.clear()
        for (var i = 0; i < list.length; i++) {
            var n = list[i]
            var ft = flowText(n)
            var parts = ft.split("·")
            rows.append({
                idx: i + 1,
                name: n.name || "",
                type: n.type || "",
                addr: n.addr || "",
                lat: latText(n),
                latKind: latKind(n),
                flow: ft,
                flowUp: (parts[0] || "").trim(),
                flowDown: (parts[1] || "").trim(),
                hasFlow: ft !== "—"
            })
        }
    }

    function isSelected(name) {
        var list = selectedNames || []
        for (var i = 0; i < list.length; i++)
            if (list[i] === name) return true
        return name === selectedName && name && name !== "—"
    }

    function rowIndexOf(name) {
        for (var i = 0; i < rows.count; i++) {
            var r = rows.get(i)
            if (r && r.name === name) return i
        }
        return -1
    }

    function pickRow(name, mods) {
        var n = nodeByName(name)
        if (!n) return
        mods = mods || 0
        var cmd = !!(mods & (Qt.ControlModifier | Qt.MetaModifier))
        var shift = !!(mods & Qt.ShiftModifier)
        var idx = rowIndexOf(name)
        if (shift && selectAnchor >= 0 && idx >= 0) {
            var lo = Math.min(selectAnchor, idx)
            var hi = Math.max(selectAnchor, idx)
            var range = []
            for (var i = lo; i <= hi; i++) {
                var r = rows.get(i)
                if (r && r.name) range.push(r.name)
            }
            selectedNames = range
        } else if (cmd) {
            var cur = (selectedNames || []).slice()
            var pos = cur.indexOf(name)
            if (pos >= 0) cur.splice(pos, 1)
            else cur.push(name)
            if (!cur.length) cur = [name]
            selectedNames = cur
            selectAnchor = idx
        } else {
            selectedNames = [name]
            selectAnchor = idx
        }
        selectedName = name
        selectedLat = latText(n)
        card.forceActiveFocus()
        nodeChosen(selectedName, selectedLat)
    }

    function selectAll() {
        var names = []
        for (var i = 0; i < rows.count; i++) {
            var r = rows.get(i)
            if (r && r.name) names.push(r.name)
        }
        selectedNames = names
        if (names.length) {
            selectedName = names[0]
            var n = nodeByName(names[0])
            selectedLat = latText(n)
            nodeChosen(selectedName, selectedLat)
        }
        selectAllDone(names.length)
        return names.length
    }

    function selectedNodeList() {
        var names = (selectedNames && selectedNames.length)
                    ? selectedNames
                    : ((selectedName && selectedName !== "—") ? [selectedName] : [])
        var out = []
        for (var i = 0; i < names.length; i++) {
            var n = nodeByName(names[i])
            if (n) out.push(n)
        }
        return out
    }

    function setLat(name, lat) {
        var n = nodeByName(name)
        if (!n) return
        n.lat = lat
        if (selectedName === name)
            selectedLat = latText(n)
        applySort()
    }

    function addFlow(name, dUp, dDown) {
        var n = nodeByName(name)
        if (!n) return
        n.flowUp = Math.max(0, Number(n.flowUp) || 0) + Math.max(0, Number(dUp) || 0)
        n.flowDown = Math.max(0, Number(n.flowDown) || 0) + Math.max(0, Number(dDown) || 0)
        n.flow = fmtBytes(n.flowUp) + "↑ · " + fmtBytes(n.flowDown) + "↓"
        applySort()
    }

    function clearLats() {
        var list = root.raw || []
        for (var i = 0; i < list.length; i++)
            list[i].lat = null
        selectedLat = latText(nodeByName(selectedName))
        applySort()
    }

    ListModel { id: rows }

    Rectangle {
        id: card
        anchors.fill: parent
        radius: 0
        color: root.surface
        border.width: 1
        border.color: root.tableBorder
        clip: true
        Accessible.role: Accessible.Table
        Accessible.name: root.t("nav.home")
        focus: true
        Keys.onPressed: function (event) {
            if ((event.modifiers & (Qt.ControlModifier | Qt.MetaModifier))
                    && !(event.modifiers & Qt.AltModifier)
                    && !(event.modifiers & Qt.ShiftModifier)
                    && event.key === Qt.Key_A) {
                event.accepted = true
                root.selectAll()
            }
        }
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.RightButton
            z: -1
            onClicked: function (mouse) {
                var g = mapToGlobal(mouse.x, mouse.y)
                root.nodeContext(g.x, g.y)
            }
        }

        Column {
            anchors.fill: parent

            Row {
                id: head
                width: parent.width
                height: 32

                Repeater {
                    model: [
                        { k: "#", key: "idx", w: root.idxW },
                        { k: "th.type", key: "type", w: root.typeW },
                        { k: "th.addr", key: "addr", w: root.addrW },
                        { k: "th.name", key: "name", w: 0 },
                        { k: "th.lat", key: "lat", w: root.latW },
                        { k: "th.flow", key: "flow", w: root.flowW }
                    ]
                    delegate: AbstractButton {
                        width: modelData.w === 0 ? root.nameW : modelData.w
                        height: 32
                        hoverEnabled: true
                        Accessible.role: Accessible.ColumnHeader
                        Accessible.name: modelData.k === "#" ? "#" : root.t(modelData.k)
                        onClicked: root.clickSort(modelData.key)
                        contentItem: Text {
                            text: (modelData.k === "#" ? "#" : root.t(modelData.k)) + (root.sortKey === modelData.key ? (root.sortDir === 1 ? " ↑" : " ↓") : "")
                            color: root.sortKey === modelData.key ? root.label : (parent.hovered ? root.secondary : root.tertiary)
                            font.family: root.fonts[0]
                            font.pixelSize: 11
                            font.weight: Font.DemiBold
                            verticalAlignment: Text.AlignVCenter
                            leftPadding: 14
                        }
                        background: Item {}
                    }
                }
            }

            Rectangle { width: parent.width; height: 1; color: root.sep }

            ListView {
                id: list
                width: parent.width
                height: parent.height - 33
                clip: true
                model: rows
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                delegate: Rectangle {
                    id: row
                    width: list.width
                    height: 34
                    property bool live: root.connected && root.connectedName.length && model.name === root.connectedName
                    property bool on: {
                        var _n = root.selectedNames
                        var _s = root.selectedName
                        return root.isSelected(model.name)
                    }
                    color: live
                           ? (area.containsMouse ? root.rowConnectedHover : (on ? root.rowConnectedSel : root.rowConnected))
                           : (on ? (area.containsMouse ? root.rowSelectedHover : root.rowSelected)
                                 : (area.containsMouse ? root.rowHover : "transparent"))

                    Row {
                        anchors.fill: parent

                        Text {
                            width: root.idxW
                            height: parent.height
                            text: model.idx
                            color: root.quaternary
                            font.family: root.fonts[0]
                            font.pixelSize: 12
                            verticalAlignment: Text.AlignVCenter
                            leftPadding: 14
                        }
                        Item {
                            width: root.typeW
                            height: parent.height
                            Rectangle {
                                visible: model.type.length > 0
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.left: parent.left
                                anchors.leftMargin: 14
                                height: 20
                                radius: 6
                                color: root.blueSoft
                                border.width: 1
                                border.color: "#14007aff"
                                implicitWidth: pillTxt.implicitWidth + 14
                                Text {
                                    id: pillTxt
                                    anchors.centerIn: parent
                                    text: model.type
                                    color: root.blue
                                    font.family: root.fonts[0]
                                    font.pixelSize: 10
                                    font.weight: Font.DemiBold
                                }
                            }
                        }
                        Text {
                            width: root.addrW
                            height: parent.height
                            text: model.addr
                            color: root.secondary
                            font.family: root.mono[0]
                            font.pixelSize: 12
                            elide: Text.ElideRight
                            verticalAlignment: Text.AlignVCenter
                            leftPadding: 14
                        }
                        Text {
                            width: root.nameW
                            height: parent.height
                            text: model.name
                            color: row.live ? root.green : root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            font.weight: row.live ? Font.DemiBold : Font.Medium
                            elide: Text.ElideRight
                            verticalAlignment: Text.AlignVCenter
                            leftPadding: 14
                        }
                        Text {
                            width: root.latW
                            height: parent.height
                            text: model.lat
                            color: root.latColor(model.latKind)
                            font.family: root.mono[0]
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                            verticalAlignment: Text.AlignVCenter
                            leftPadding: 14
                        }
                        Row {
                            width: root.flowW
                            height: parent.height
                            leftPadding: 14
                            spacing: 0
                            Text {
                                visible: !model.hasFlow
                                text: "—"
                                color: root.quaternary
                                font.family: root.mono[0]
                                font.pixelSize: 12
                                anchors.verticalCenter: parent.verticalCenter
                            }
                            Text {
                                visible: model.hasFlow
                                text: model.flowUp
                                color: "#0a84ff"
                                font.family: root.mono[0]
                                font.pixelSize: 12
                                anchors.verticalCenter: parent.verticalCenter
                                elide: Text.ElideRight
                            }
                            Text {
                                visible: model.hasFlow && model.flowDown.length
                                text: " · "
                                color: root.secondary
                                font.family: root.mono[0]
                                font.pixelSize: 12
                                anchors.verticalCenter: parent.verticalCenter
                            }
                            Text {
                                visible: model.hasFlow && model.flowDown.length
                                text: model.flowDown
                                color: th && th.purple ? th.purple : "#af52de"
                                font.family: root.mono[0]
                                font.pixelSize: 12
                                anchors.verticalCenter: parent.verticalCenter
                                elide: Text.ElideRight
                            }
                        }
                    }

                    MouseArea {
                        id: area
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: function (mouse) {
                            if (mouse.button === Qt.RightButton) {
                                if (!root.isSelected(model.name))
                                    root.pickRow(model.name, 0)
                                var g = mapToGlobal(mouse.x, mouse.y)
                                root.nodeContext(g.x, g.y)
                            } else {
                                root.pickRow(model.name, mouse.modifiers)
                            }
                        }
                        onDoubleClicked: function (mouse) {
                            if (mouse.button === Qt.LeftButton)
                                root.nodeEdit(model.name)
                        }
                    }
                }
            }
        }
    }
}
