pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window

Item {
    id: root
    height: open ? openH : collapsedH
    property bool open: false
    property bool resizing: false
    property int openH: 132
    property int collapsedH: th && th.dockCollapsedH ? th.dockCollapsedH : 32
    property string panel: "log"
    property int minOpenH: 80
    readonly property int maxOpenH: Math.max(minOpenH + 40, Math.floor(parent ? parent.height * 0.72 : 360))

    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
    readonly property var mono: th ? th.monoFamilies : ["Menlo"]
    readonly property color label: th ? th.label : "#1d1d1f"
    readonly property color secondary: th ? th.secondary : "#6e6e73"
    readonly property color tertiary: th ? th.tertiary : "#8e8e93"
    readonly property color quaternary: th ? th.quaternary : "#aeaeb2"
    readonly property color blue: th ? th.blue : "#007aff"
    readonly property color blueSoft: th ? th.blueSoft : "#1e007aff"
    readonly property color green: th ? th.green : "#34c759"
    readonly property color orange: th ? th.orange : "#ff9f0a"
    readonly property color fill: th ? th.fill : "#1e787880"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color chrome: th ? th.chromeSolid : "#fafafc"
    readonly property color controlBg: th ? th.controlBg : "#ffffff"
    readonly property color controlText: th ? th.controlText : "#000000"

    property string filterText: ""
    property string filterLvl: "all"

    function t(k, v) { return i18 ? i18.t(k, v) : k }

    function clampH(px) {
        var n = Math.round(px)
        if (n < minOpenH) n = minOpenH
        if (n > maxOpenH) n = maxOpenH
        return n
    }

    function setOpen(on) { open = !!on }

    function setPanel(id) {
        panel = id
        setOpen(true)
    }

    property int connSel: -1
    property string connSelDest: ""
    property var lastConns: []
    property string connSortKey: ""
    property int connSortDir: 1

    onFilterTextChanged: rebuildLogView()
    onFilterLvlChanged: rebuildLogView()

    function copyText(s) {
        // nexus is an intentionally injected C++ context property.
        // qmllint disable unqualified
        if (typeof nexus === "undefined" || !nexus || typeof nexus.setClipboardText !== "function")
            return false
        nexus.setClipboardText(String(s || ""))
        // qmllint enable unqualified
        return true
    }

    function allVisibleLogText() {
        var out = []
        for (var i = 0; i < logs.count; i++) {
            var m = logs.get(i)
            if (!m) continue
            if (!lineVisible(m.lvl, m.msg)) continue
            out.push(m.time + "  " + m.tag + "  " + m.msg)
        }
        return out.join("\n")
    }

    function selectedLogText() {
        var s = logView && logView.selectedText
        return s && s.length ? s : allVisibleLogText()
    }

    function copyLogs() {
        var s = selectedLogText()
        if (copyText(s)) appendLog("SYS", "ok", t("log.copy"))
    }

    function selectAllLogs() {
        logView.selectAll()
        copyLogs()
    }

    function logColor(cls) {
        if (cls === "ok") return root.green
        if (cls === "warn") return root.orange
        return root.blue
    }

    function css(c) {
        var s = String(c)
        if (s.length === 9 && s[0] === "#") return "#" + s.substring(3)
        return s
    }

    function logLineHtml(m) {
        return "<span style='color:" + css(root.quaternary) + "'>" + esc(m.time)
             + "</span>  <span style='color:" + css(logColor(m.cls)) + ";font-weight:600'>" + esc(m.tag)
             + "</span>  <span style='color:" + css(root.secondary) + "'>" + esc(m.msg)
             + "</span><br/>"
    }

    function rebuildLogView() {
        if (!logView) return
        var html = ""
        for (var i = 0; i < logs.count; i++) {
            var m = logs.get(i)
            if (!m || !lineVisible(m.lvl, m.msg)) continue
            html += logLineHtml(m)
        }
        logView.text = html
        scrollLogEnd()
    }

    function scrollLogEnd() {
        Qt.callLater(function () {
            if (!logFlick) return
            logFlick.contentY = Math.max(0, logFlick.contentHeight - logFlick.height)
        })
    }

    function esc(s) {
        return String(s || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    }

    function popupAt(menu, item, mx, my) {
        if (!menu || !item) return
        var g = item.mapToGlobal(mx, my)
        var p = root.mapFromGlobal(g.x, g.y)
        menu.x = p.x
        menu.y = p.y
        menu.open()
    }

    function connRowText(i) {
        if (i < 0 || i >= conns.count) return ""
        var m = conns.get(i)
        if (!m) return ""
        return [m.time, m.app, m.pid, m.dest, m.proto, m.outbound, m.flow].join("\t")
    }

    function copyConnHost() {
        if (connSel < 0) return
        var m = conns.get(connSel)
        if (!m) return
        var d = String(m.dest || "").trim()
        if (!d || d === "—") return
        copyText(d)
    }

    function copyConnRow() {
        if (connSel < 0) return
        copyText(connRowText(connSel))
    }

    function now() {
        var d = new Date()
        function p(x) { return (x < 10 ? "0" : "") + x }
        return p(d.getHours()) + ":" + p(d.getMinutes()) + ":" + p(d.getSeconds())
    }

    function appendLog(tag, cls, msg) {
        var lvl = (cls === "ok" || cls === "warn" || cls === "info") ? cls : "info"
        var row = {
            time: now(),
            tag: String(tag || ""),
            cls: String(cls || "info"),
            lvl: lvl,
            msg: String(msg || "")
        }
        logs.append(row)
        if (logView && lineVisible(lvl, row.msg)) {
            logView.text += logLineHtml(row)
            scrollLogEnd()
        }
    }

    function clearLog() {
        logs.clear()
        if (logView) logView.text = ""
        appendLog("SYS", "info", t("log.cleared"))
    }

    function lineVisible(lvl, msg) {
        if (filterLvl !== "all" && lvl !== filterLvl) return false
        if (filterText.length && String(msg).toLowerCase().indexOf(filterText.toLowerCase()) < 0
                && String(lvl).indexOf(filterText.toLowerCase()) < 0)
            return false
        return true
    }

    function fmtBytes(n) {
        n = Number(n) || 0
        if (n < 1024) return n + " B"
        if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB"
        if (n < 1024 * 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + " MB"
        return (n / (1024 * 1024 * 1024)).toFixed(2) + " GB"
    }

    function fmtTime(ms) {
        var n = Number(ms) || 0
        if (n <= 0) return "—"
        if (n < 1e11) n *= 1000
        var d = new Date(n)
        if (isNaN(d.getTime())) return "—"
        function p(x) { return (x < 10 ? "0" : "") + x }
        return p(d.getHours()) + ":" + p(d.getMinutes()) + ":" + p(d.getSeconds())
    }

    function connKey(c) {
        var id = String((c && c.id) != null ? c.id : "").trim()
        if (id.length) return id
        var pid = Number(c && c.process_id) || 0
        return "_" + (Number(c && c.created_at) || 0) + "|" + ((c && (c.domain || c.dest)) || "") + "|" + ((c && c.process) || "") + "|" + pid
    }

    function connSortVal(c, key) {
        if (key === "time") return Number(c && c.created_at) || 0
        if (key === "app") return String((c && c.process) || "").toLowerCase()
        if (key === "pid") return Number(c && c.process_id) || 0
        if (key === "dest") return String((c && (c.domain || c.dest)) || "").toLowerCase()
        if (key === "proto") return String((c && (c.network || c.protocol)) || "").toLowerCase()
        if (key === "out") return String((c && c.outbound) || "").toLowerCase()
        if (key === "flow") return (Number(c && c.upload) || 0) + (Number(c && c.download) || 0)
        return ""
    }

    function clickConnSort(key) {
        if (!key) return
        if (connSortKey === key) connSortDir = -connSortDir
        else { connSortKey = key; connSortDir = 1 }
        setConns(lastConns)
    }

    function connHdrMark(key) {
        if (connSortKey !== key) return ""
        return connSortDir === 1 ? " ↑" : " ↓"
    }

    function setConns(list) {
        var raw = Array.isArray(list) ? list.slice() : []
        lastConns = raw
        var seen = ({})
        var out = []
        for (var i = 0; i < raw.length; i++) {
            var c = raw[i] || {}
            var k = connKey(c)
            seen[k] = c
        }
        for (var key in seen)
            out.push({ key: key, c: seen[key] })
        if (connSortKey) {
            var dir = connSortDir
            var sk = connSortKey
            out.sort(function (a, b) {
                var va = connSortVal(a.c, sk)
                var vb = connSortVal(b.c, sk)
                var cmp
                if (typeof va === "number" && typeof vb === "number") cmp = va - vb
                else {
                    va = String(va)
                    vb = String(vb)
                    cmp = va < vb ? -1 : (va > vb ? 1 : 0)
                }
                return cmp * dir
            })
        }
        conns.clear()
        for (var j = 0; j < out.length; j++) {
            var x = out[j].c
            var proto = x.network || ""
            if (x.protocol) proto = proto ? (proto + " (" + x.protocol + ")") : x.protocol
            if (!proto) proto = "—"
            var up = Number(x.upload) || 0
            var down = Number(x.download) || 0
            conns.append({
                time: fmtTime(x.created_at),
                app: x.process || "—",
                pid: (Number(x.process_id) || 0) > 0 ? String(x.process_id) : "—",
                dest: x.domain || x.dest || "—",
                proto: proto,
                outbound: x.outbound || "—",
                flow: fmtBytes(up) + "↑ " + fmtBytes(down) + "↓"
            })
        }
        var keep = connSelDest
        connSel = -1
        if (keep) {
            for (var k = 0; k < conns.count; k++) {
                if (conns.get(k).dest === keep) {
                    connSel = k
                    break
                }
            }
        }
    }

    Behavior on height {
        enabled: !root.resizing
        NumberAnimation { duration: 200; easing.type: Easing.OutCubic }
    }

    ListModel { id: logs }
    ListModel { id: conns }

    Rectangle {
        anchors.fill: parent
        color: root.chrome
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 1
        color: root.sep
        visible: root.open
    }

    MouseArea {
        id: resizer
        visible: root.open
        anchors.left: parent.left
        anchors.right: parent.right
        y: -3
        height: 6
        cursorShape: Qt.SizeVerCursor
        hoverEnabled: true
        z: 5
        Accessible.name: root.t("title.dockResize")
        property real startY: 0
        property int startH: 0
        Rectangle {
            anchors.fill: parent
            color: resizer.pressed || resizer.containsMouse ? "#38007aff" : "transparent"
        }
        onPressed: function (mouse) {
            root.resizing = true
            var p = mapToItem(null, mouse.x, mouse.y)
            startY = p.y
            startH = root.openH
        }
        onPositionChanged: function (mouse) {
            if (!pressed) return
            var p = mapToItem(null, mouse.x, mouse.y)
            root.openH = root.clampH(startH + (startY - p.y))
        }
        onReleased: root.resizing = false
    }

    Column {
        anchors.fill: parent

        Item {
            id: bar
            width: parent.width
            height: root.collapsedH

            Row {
                anchors.left: parent.left
                anchors.leftMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2

                Repeater {
                    model: [
                        { id: "log", key: "dock.log" },
                        { id: "conn", key: "dock.conn" }
                    ]
                    delegate: AbstractButton {
                        id: tab
                        required property var modelData
                        height: root.collapsedH
                        hoverEnabled: true
                        Accessible.name: root.t(tab.modelData.key)
                        Accessible.role: Accessible.PageTab
                        onClicked: root.setPanel(tab.modelData.id)
                        property bool on: root.panel === tab.modelData.id
                        contentItem: Text {
                            text: root.t(tab.modelData.key)
                            color: tab.on ? root.label : (tab.hovered ? root.secondary : root.tertiary)
                            font.family: root.fonts[0]
                            font.pixelSize: 12
                            font.weight: tab.on ? Font.DemiBold : Font.Medium
                            leftPadding: 10
                            rightPadding: 10
                            verticalAlignment: Text.AlignVCenter
                        }
                        background: Item {
                            Rectangle {
                                visible: tab.on
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                height: 2
                                color: root.label
                            }
                        }
                    }
                }
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2

                AbstractButton {
                    id: clearBtn
                    width: 26
                    height: 26
                    hoverEnabled: true
                    Accessible.name: root.t("title.logClear")
                    Tip { text: root.t("title.logClear") }
                    onClicked: root.clearLog()
                    background: Rectangle {
                        radius: 6
                        color: clearBtn.hovered ? root.fill : "transparent"
                    }
                    contentItem: Canvas {
                        property bool hot: clearBtn.hovered
                        onHotChanged: requestPaint()
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.clearRect(0, 0, width, height)
                            ctx.strokeStyle = hot ? root.label : root.tertiary
                            ctx.lineWidth = 1.6
                            ctx.lineCap = "round"
                            ctx.beginPath()
                            ctx.moveTo(6, 8); ctx.lineTo(20, 8)
                            ctx.moveTo(10, 8); ctx.lineTo(10, 6.5)
                            ctx.lineTo(16, 6.5); ctx.lineTo(16, 8)
                            ctx.moveTo(8, 8); ctx.lineTo(9, 19)
                            ctx.arcTo(9, 21, 11, 21, 2)
                            ctx.lineTo(15, 21)
                            ctx.arcTo(17, 21, 17, 19, 2)
                            ctx.lineTo(18, 8)
                            ctx.stroke()
                        }
                    }
                }

                AbstractButton {
                    id: toggle
                    width: 26
                    height: 26
                    hoverEnabled: true
                    Accessible.name: root.open ? root.t("title.dockCollapse") : root.t("title.dockExpand")
                    Accessible.checked: root.open
                    Tip { text: root.open ? root.t("title.dockCollapse") : root.t("title.dockExpand") }
                    onClicked: root.setOpen(!root.open)
                    background: Rectangle {
                        radius: 6
                        color: toggle.hovered ? root.fill : "transparent"
                    }
                    contentItem: Canvas {
                        rotation: root.open ? 180 : 0
                        Behavior on rotation { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
                        property bool hot: toggle.hovered
                        onHotChanged: requestPaint()
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.clearRect(0, 0, width, height)
                            ctx.strokeStyle = hot ? root.label : root.tertiary
                            ctx.lineWidth = 1.6
                            ctx.lineCap = "round"
                            ctx.lineJoin = "round"
                            ctx.beginPath()
                            ctx.moveTo(7, 10)
                            ctx.lineTo(13, 16)
                            ctx.lineTo(19, 10)
                            ctx.stroke()
                        }
                    }
                }
            }
        }

        Item {
            width: parent.width
            height: Math.max(0, parent.height - bar.height)
            visible: root.open
            clip: true

            Column {
                visible: root.panel === "log"
                anchors.fill: parent

                Row {
                    id: filterBar
                    width: parent.width
                    height: 28
                    leftPadding: 10
                    rightPadding: 10
                    spacing: 4

                    TextField {
                        id: filterInput
                        width: Math.max(80, parent.width - 10 - 4 * 52 - 20)
                        height: 22
                        anchors.verticalCenter: parent.verticalCenter
                        placeholderText: root.t("log.filterPlaceholder")
                        color: root.controlText
                        font.family: root.fonts[0]
                        font.pixelSize: 12
                        background: Rectangle {
                            radius: 5
                            color: root.controlBg
                            border.width: 1
                            border.color: filterInput.activeFocus ? root.blue : root.sep
                        }
                        onTextChanged: root.filterText = text
                    }

                    Repeater {
                        model: [
                            { id: "all", label: root.t("log.filterAll") },
                            { id: "info", label: "INFO" },
                            { id: "ok", label: "OK" },
                            { id: "warn", label: "WARN" }
                        ]
                        delegate: AbstractButton {
                            id: filterButton
                            required property var modelData
                            height: 22
                            hoverEnabled: true
                            onClicked: root.filterLvl = filterButton.modelData.id
                            property bool on: root.filterLvl === filterButton.modelData.id
                            contentItem: Text {
                                text: filterButton.modelData.label
                                color: filterButton.on ? root.blue : root.secondary
                                font.family: root.fonts[0]
                                font.pixelSize: 11
                                font.weight: Font.Medium
                                leftPadding: 7
                                rightPadding: 7
                                verticalAlignment: Text.AlignVCenter
                            }
                            background: Rectangle {
                                radius: 5
                                color: filterButton.on ? root.blueSoft : (filterButton.hovered ? root.fill : "transparent")
                            }
                        }
                    }
                }

                Rectangle { width: parent.width; height: 1; color: root.sep }

                Flickable {
                    id: logFlick
                    width: parent.width
                    height: parent.height - 29
                    clip: true
                    interactive: false
                    contentWidth: width
                    contentHeight: logView.implicitHeight
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                    WheelHandler {
                        onWheel: function (w) {
                            var next = logFlick.contentY - w.angleDelta.y / 8
                            var maxY = Math.max(0, logFlick.contentHeight - logFlick.height)
                            logFlick.contentY = Math.max(0, Math.min(maxY, next))
                        }
                    }
                    TextEdit {
                        id: logView
                        width: logFlick.width
                        leftPadding: 12
                        rightPadding: 12
                        topPadding: 4
                        bottomPadding: 4
                        textFormat: TextEdit.RichText
                        readOnly: true
                        selectByMouse: true
                        persistentSelection: true
                        wrapMode: TextEdit.Wrap
                        color: root.secondary
                        font.family: root.mono[0]
                        font.pixelSize: 12
                        TapHandler {
                            acceptedButtons: Qt.RightButton
                            gesturePolicy: TapHandler.ReleaseWithinBounds
                            onTapped: function (eventPoint) {
                                root.popupAt(logCtx, logView, eventPoint.position.x, eventPoint.position.y)
                            }
                        }
                    }
                }
            }

            ListView {
                visible: root.panel === "conn"
                id: connList
                anchors.fill: parent
                clip: true
                model: conns
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                header: Row {
                    width: connList.width
                    height: 24
                    Repeater {
                        model: [
                            { key: "time", w: 80, k: "th.time" },
                            { key: "app", w: 0, k: "th.proc" },
                            { key: "pid", w: 56, k: "PID" },
                            { key: "dest", w: 0, k: "th.dest" },
                            { key: "proto", w: 72, k: "th.proto" },
                            { key: "out", w: 60, k: "th.outbound" },
                            { key: "flow", w: 130, k: "th.flow" }
                        ]
                        delegate: AbstractButton {
                            id: connHeader
                            required property var modelData
                            width: connHeader.modelData.w === 0
                                   ? Math.max(70, (connList.width - 80 - 56 - 72 - 60 - 130) / 2)
                                   : connHeader.modelData.w
                            height: 24
                            hoverEnabled: true
                            Accessible.role: Accessible.ColumnHeader
                            Accessible.name: connHeader.modelData.k === "PID" ? "PID" : root.t(connHeader.modelData.k)
                            onClicked: root.clickConnSort(connHeader.modelData.key)
                            contentItem: Text {
                                text: (connHeader.modelData.k === "PID" ? "PID" : root.t(connHeader.modelData.k)) + root.connHdrMark(connHeader.modelData.key)
                                color: root.connSortKey === connHeader.modelData.key ? root.label : (connHeader.hovered ? root.secondary : root.tertiary)
                                font.family: root.fonts[0]
                                font.pixelSize: 11
                                font.weight: Font.DemiBold
                                leftPadding: 8
                                verticalAlignment: Text.AlignVCenter
                            }
                            background: Item {}
                        }
                    }
                }
                delegate: Item {
                    id: connRow
                    required property int index
                    required property string time
                    required property string app
                    required property string pid
                    required property string dest
                    required property string proto
                    required property string outbound
                    required property string flow
                    width: connList.width
                    height: 22
                    property bool on: root.connSel === connRow.index
                    Rectangle {
                        anchors.fill: parent
                        color: connRow.on ? root.blueSoft : "transparent"
                    }
                    Row {
                        anchors.fill: parent
                        Text { width: 80; text: connRow.time; color: root.quaternary; font.family: root.mono[0]; font.pixelSize: 12; leftPadding: 8; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter; height: 22 }
                        Text { width: Math.max(70, (connList.width - 80 - 56 - 72 - 60 - 130) / 2); text: connRow.app; color: root.label; font.family: root.fonts[0]; font.pixelSize: 12; font.weight: Font.Medium; leftPadding: 8; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter; height: 22 }
                        Text { width: 56; text: connRow.pid; color: root.quaternary; font.family: root.mono[0]; font.pixelSize: 12; leftPadding: 8; horizontalAlignment: Text.AlignRight; rightPadding: 8; verticalAlignment: Text.AlignVCenter; height: 22 }
                        Text { width: Math.max(70, (connList.width - 80 - 56 - 72 - 60 - 130) / 2); text: connRow.dest; color: root.secondary; font.family: root.mono[0]; font.pixelSize: 12; leftPadding: 8; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter; height: 22 }
                        Text { width: 72; text: connRow.proto; color: root.secondary; font.family: root.mono[0]; font.pixelSize: 12; leftPadding: 8; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter; height: 22 }
                        Text { width: 60; text: connRow.outbound; color: root.blue; font.family: root.fonts[0]; font.pixelSize: 11; leftPadding: 8; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter; height: 22 }
                        Text { width: 130; text: connRow.flow; color: root.secondary; font.family: root.mono[0]; font.pixelSize: 12; leftPadding: 8; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter; height: 22 }
                    }
                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: function (mouse) {
                            root.connSel = connRow.index
                            root.connSelDest = String(connRow.dest || "")
                            if (mouse.button === Qt.RightButton)
                                root.popupAt(connCtx, this, mouse.x, mouse.y)
                        }
                    }
                }
            }
        }
    }

    Shortcut {
        sequences: [ StandardKey.Copy ]
        enabled: root.open && root.panel === "log"
        onActivated: root.copyLogs()
    }

    component DockItem: AbstractButton {
        id: dockItem
        property var act: function () {}
        width: parent ? parent.width : 180
        height: 28
        hoverEnabled: true
        onClicked: {
            logCtx.close()
            connCtx.close()
            dockItem.act()
        }
        background: Rectangle {
            radius: 4
            color: dockItem.hovered && dockItem.enabled ? root.fill : "transparent"
        }
        contentItem: Text {
            text: dockItem.text
            color: dockItem.enabled ? root.label : root.tertiary
            font.family: root.fonts[0]
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
            leftPadding: 8
            opacity: dockItem.enabled ? 1 : 0.45
        }
    }

    Popup {
        id: logCtx
        padding: 6
        modal: false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle {
            radius: 12
            color: root.controlBg
            border.color: root.sep
            border.width: 1
        }
        contentItem: Column {
            width: 180
            spacing: 0
            DockItem { text: root.t("log.copy"); act: function () { root.copyLogs() } }
            DockItem { text: root.t("ctx.selectAll"); act: function () { root.selectAllLogs() } }
            DockItem { text: root.t("log.clear"); act: function () { root.clearLog() } }
        }
    }

    Popup {
        id: connCtx
        padding: 6
        modal: false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle {
            radius: 12
            color: root.controlBg
            border.color: root.sep
            border.width: 1
        }
        contentItem: Column {
            width: 180
            spacing: 0
            DockItem { text: root.t("conn.copyHost"); enabled: root.connSel >= 0; act: function () { root.copyConnHost() } }
            DockItem { text: root.t("conn.copyRow"); enabled: root.connSel >= 0; act: function () { root.copyConnRow() } }
        }
    }
}
