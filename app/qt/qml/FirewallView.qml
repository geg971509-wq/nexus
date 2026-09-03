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
    readonly property var mono: th ? th.monoFamilies : ["Menlo"]
    readonly property color bg: th ? th.bg : "#f5f5f7"
    readonly property color label: th ? th.label : "#1d1d1f"
    readonly property color secondary: th ? th.secondary : "#6e6e73"
    readonly property color tertiary: th ? th.tertiary : "#8e8e93"
    readonly property color blue: th ? th.blue : "#007aff"
    readonly property color surface: th ? th.surface : "#ffffff"
    readonly property color chrome: th ? th.chromeSolid : "#fafafc"
    readonly property color sep: th ? th.separator : "#1e3c3c43"
    readonly property color hairline: th ? th.hairline : "#0b000000"
    readonly property color fill: th ? th.fill : "#1e787880"
    readonly property color fill2: th ? th.fill2 : "#14787880"
    readonly property color greenSoft: th ? th.greenSoft : "#2434c759"
    readonly property color latGood: th ? th.latGood : "#248a3d"
    readonly property color orange: th ? th.orange : "#ff9f0a"
    readonly property color red: th ? th.red : "#ff3b30"
    readonly property int rLg: th ? th.radiusLg : 14
    readonly property bool dark: th ? th.dark : false
    readonly property color sectionLabel: dark ? Qt.rgba(0.922, 0.922, 0.961, 0.48)
                                               : Qt.rgba(0.235, 0.235, 0.263, 0.58)
    readonly property color helpBg: dark ? "#1a98989d" : "#0d787880"
    readonly property color pillWarnBg: "#24ff9f0a"
    readonly property color pillWarnFg: dark ? "#ffb340" : "#c77a00"
    readonly property color pillWarnBd: "#47ff9f0a"
    readonly property color pillErrBg: "#1fff3b30"
    readonly property color pillErrBd: "#38ff3b30"
    readonly property color pillOkBd: "#3834c759"
    readonly property color rowHover: th ? th.sideHover : "#09000000"

    property bool busy: false
    property bool hasApi: false
    property bool helperRunning: false
    property bool helperInstalled: false
    property var requestId: null
    property string actionError: ""

    property string stateText: "—"
    property string stateTone: "neutral"
    property string supportText: "—"
    property string supportTone: "muted"
    property string policyText: "—"
    property string policyTone: "muted"
    property string desiredText: "—"
    property string desiredTone: "muted"
    property string appliedText: "—"
    property string appliedTone: "muted"
    property string peerText: "—"
    property string peerTone: "muted"
    property string tunText: "—"
    property string tunTone: "muted"
    property string errText: "—"
    property string errTone: "muted"
    property string helperText: "—"
    property string helperTone: "neutral"

    function t(k, v) { return i18 ? i18.t(k, v) : k }

    function api() {
        // nexus is an intentionally injected C++ context property.
        // qmllint disable unqualified
        return (typeof nexus === "undefined") ? null : nexus
        // qmllint enable unqualified
    }

    function empty(v) { return v === undefined || v === null || v === "" }

    function invoke(cmd) {
        var n = api()
        if (!n || typeof n.invoke !== "function")
            return { _missing: true }
        try {
            var raw = n.invoke(cmd, "{}")
            var obj = raw
            if (typeof raw === "string")
                obj = JSON.parse(raw)
            return obj || {}
        } catch (e) {
            return { ok: false, error: String(e) }
        }
    }

    function toneColor(kind, pill) {
        if (kind === "ok") return root.latGood
        if (kind === "warn") return pill ? root.pillWarnFg : root.orange
        if (kind === "err") return root.red
        if (kind === "muted") return root.tertiary
        return pill ? root.secondary : root.label
    }

    function pillBg(kind) {
        if (kind === "ok") return root.greenSoft
        if (kind === "warn") return root.pillWarnBg
        if (kind === "err") return root.pillErrBg
        return root.fill
    }

    function pillBd(kind) {
        if (kind === "ok") return root.pillOkBd
        if (kind === "warn") return root.pillWarnBd
        if (kind === "err") return root.pillErrBd
        return root.sep
    }

    function paint(st) {
        if (!st || typeof st !== "object") {
            stateText = "—"; stateTone = "neutral"
            supportText = "—"; supportTone = "muted"
            policyText = "—"; policyTone = "muted"
            desiredText = "—"; desiredTone = "muted"
            appliedText = "—"; appliedTone = "muted"
            peerText = "—"; peerTone = "muted"
            tunText = "—"; tunTone = "muted"
            errText = "—"; errTone = "muted"
            helperText = "—"; helperTone = "neutral"
            helperRunning = false
            helperInstalled = false
            return
        }
        var stateRaw = String(st.tunnel_state || "").toLowerCase()
        var sTone = "neutral"
        if (stateRaw === "connected") sTone = "ok"
        else if (stateRaw === "error" || stateRaw === "blocked") sTone = "err"
        else if (stateRaw === "connecting" || stateRaw === "disconnecting") sTone = "warn"
        stateText = empty(st.tunnel_state) ? "—" : String(st.tunnel_state)
        stateTone = empty(st.tunnel_state) ? "neutral" : sTone

        var support = st.support === "active" ? t("fw.active") : st.support
        supportText = empty(support) ? "—" : String(support)
        supportTone = empty(support) ? "muted"
                    : (st.support === "active" ? "ok" : "warn")

        var mismatch = !!st.policy_mismatch
        var pol = mismatch
            ? ((st.desired_policy || "—") + " ≠ " + (st.applied_policy || st.last_policy || "—"))
            : (st.last_policy || st.applied_policy || "—")
        policyText = empty(pol) ? "—" : String(pol)
        policyTone = mismatch ? "warn" : (st.last_policy ? "" : "muted")

        desiredText = empty(st.desired_policy) ? "—" : String(st.desired_policy)
        desiredTone = empty(st.desired_policy) ? "muted" : (mismatch ? "warn" : "")
        var applied = st.applied_policy || st.last_policy
        appliedText = empty(applied) ? "—" : String(applied)
        appliedTone = empty(applied) ? "muted" : (mismatch ? "warn" : "")
        peerText = empty(st.peer) ? "—" : String(st.peer)
        peerTone = empty(st.peer) ? "muted" : ""
        tunText = empty(st.tun_if) ? "—" : String(st.tun_if)
        tunTone = empty(st.tun_if) ? "muted" : ""
        var shownError = actionError || st.last_error
        errText = empty(shownError) ? "—" : String(shownError)
        errTone = empty(shownError) ? "muted" : "err"

        helperRunning = !!st.helper_running
        helperInstalled = !!st.helper_installed
        var h = helperRunning ? t("fw.helperOn")
              : (helperInstalled ? t("fw.helperOff") : t("fw.helperMissing"))
        helperText = st.helper_detail ? (h + " · " + st.helper_detail) : h
        helperTone = helperRunning ? "ok" : (helperInstalled ? "warn" : "err")
    }

    function unwrap(r) {
        if (!r || r._missing) return null
        if (r.offline) return { last_error: "backend offline" }
        if (r.ok === false) return { last_error: r.error || "firewall_status failed" }
        if (r.data && typeof r.data === "object") return r.data
        return r
    }

    function refresh() {
        if (busy) return
        var n = api()
        hasApi = !!(n && typeof n.invoke === "function")
        if (!hasApi) {
            paint(null)
            return
        }
        var started = invoke("firewall_status")
        if (!started || started._missing || started.ok === false || started.request_id == null) {
            paint(unwrap(started))
            return
        }
        requestId = started.request_id
    }

    function runHelper(cmd) {
        if (busy || !hasApi) return
        busy = true
        actionError = ""
        var started = invoke(cmd)
        if (!started || started._missing || started.ok === false || started.request_id == null) {
            busy = false
            if (started && started._missing) {
                paint(null)
                return
            }
            actionError = (started && started.error) || "firewall helper failed"
            errText = actionError
            errTone = "err"
            return
        }
        requestId = started.request_id
    }

    function onFirewallResult(r) {
        if (!r || requestId == null || r.request_id !== requestId) return
        if (r.op !== "status") busy = false
        if (r.ok === false || r.error) {
            if (r.op === "status") {
                paint({ last_error: r.error || "firewall_status failed" })
                return
            }
            actionError = r.error || "firewall helper failed"
            refresh()
            return
        }
        if (r.op !== "status") actionError = ""
        paint(r)
    }

    Connections {
        target: root.api()
        function onEvent(name, json) {
            if (name !== "firewall-result") return
            var r = json
            if (typeof json === "string") {
                try { r = JSON.parse(json) } catch (e) { return }
            }
            if (r && r.payload !== undefined) r = r.payload
            root.onFirewallResult(r)
        }
    }

    onVisibleChanged: if (visible) refresh()
    Component.onCompleted: if (visible) refresh()

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
                text: root.t("fw.title")
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
            contentHeight: rail.height + 52
            boundsBehavior: Flickable.StopAtBounds

            Item {
                id: rail
                width: Math.min(680, flick.width - 48)
                x: Math.round((flick.width - width) / 2)
                y: 20
                height: body.height

                Column {
                    id: body
                    width: parent.width
                    spacing: 16

                    Rectangle {
                        width: parent.width
                        height: statusCol.height
                        radius: root.rLg
                        color: root.surface
                        border.width: 1
                        border.color: root.hairline
                        clip: true

                        Column {
                            id: statusCol
                            width: parent.width
                            Text {
                                width: parent.width
                                height: 32
                                text: root.t("fw.secStatus")
                                color: root.sectionLabel
                                font.family: root.fonts[0]
                                font.pixelSize: 11
                                font.weight: Font.Medium
                                leftPadding: 16
                                topPadding: 10
                                verticalAlignment: Text.AlignVCenter
                            }
                            FwRow { labelKey: "fw.state"; value: root.stateText; tone: root.stateTone; pill: true }
                            FwRow { labelKey: "fw.support"; value: root.supportText; tone: root.supportTone }
                            FwRow { labelKey: "fw.policy"; value: root.policyText; tone: root.policyTone; mono: true }
                            FwRow { labelKey: "fw.desired"; value: root.desiredText; tone: root.desiredTone; mono: true }
                            FwRow { labelKey: "fw.applied"; value: root.appliedText; tone: root.appliedTone; mono: true }
                            FwRow { labelKey: "fw.peer"; value: root.peerText; tone: root.peerTone; mono: true }
                            FwRow { labelKey: "fw.tun"; value: root.tunText; tone: root.tunTone; mono: true }
                            FwRow { labelKey: "fw.error"; value: root.errText; tone: root.errTone; last: true }
                        }
                    }

                    Rectangle {
                        width: parent.width
                        height: helperCol.height
                        radius: root.rLg
                        color: root.surface
                        border.width: 1
                        border.color: root.hairline
                        clip: true

                        Column {
                            id: helperCol
                            width: parent.width
                            Text {
                                width: parent.width
                                height: 32
                                text: root.t("fw.secHelper")
                                color: root.sectionLabel
                                font.family: root.fonts[0]
                                font.pixelSize: 11
                                font.weight: Font.Medium
                                leftPadding: 16
                                topPadding: 10
                                verticalAlignment: Text.AlignVCenter
                            }
                            FwRow { labelKey: "fw.helper"; value: root.helperText; tone: root.helperTone; pill: true }
                            Item {
                                id: actionRow
                                width: parent.width
                                height: Math.max(44, btns.height + 24)
                                Rectangle {
                                    anchors.fill: parent
                                    color: actionHover.hovered ? root.rowHover : "transparent"
                                }
                                HoverHandler { id: actionHover }
                                Rectangle {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    height: 1
                                    color: root.sep
                                }
                                Text {
                                    anchors.left: parent.left
                                    anchors.leftMargin: 16
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 140
                                    text: root.t("fw.actions")
                                    color: root.label
                                    font.family: root.fonts[0]
                                    font.pixelSize: 13
                                    font.weight: Font.Medium
                                    elide: Text.ElideRight
                                }
                                Row {
                                    id: btns
                                    anchors.right: parent.right
                                    anchors.rightMargin: 16
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 8
                                    FwBtn {
                                        text: root.t("fw.install")
                                        primary: true
                                        enabled: root.hasApi && !root.busy && !root.helperRunning
                                        Accessible.name: text
                                        onClicked: root.runHelper("firewall_helper_install")
                                    }
                                    FwBtn {
                                        text: root.t("fw.uninstall")
                                        primary: false
                                        enabled: root.hasApi && !root.busy && (root.helperInstalled || root.helperRunning)
                                        Accessible.name: text
                                        onClicked: root.runHelper("firewall_helper_uninstall")
                                    }
                                }
                            }
                        }
                    }

                    Rectangle {
                        width: parent.width
                        implicitHeight: helpTxt.implicitHeight + 24
                        radius: 10
                        color: root.helpBg
                        border.width: 1
                        border.color: root.hairline
                        Text {
                            id: helpTxt
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 14
                            text: root.t("fw.help")
                            color: root.secondary
                            font.family: root.fonts[0]
                            font.pixelSize: 12
                            font.weight: Font.Normal
                            wrapMode: Text.Wrap
                            lineHeight: 1.5
                            lineHeightMode: Text.ProportionalHeight
                        }
                    }
                }
            }
        }
    }

    component FwRow: Item {
        id: row
        property string labelKey: ""
        property string value: "—"
        property string tone: "muted"
        property bool mono: false
        property bool pill: false
        property bool last: false
        width: parent ? parent.width : 0
        height: Math.max(44, innerH + 24)
        property int innerH: pill ? pillBox.height : val.implicitHeight

        Rectangle {
            anchors.fill: parent
            color: rowHoverH.hovered ? root.rowHover : "transparent"
            radius: row.last ? root.rLg : 0
            Rectangle {
                visible: row.last
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: parent.radius
                color: parent.color
            }
        }
        HoverHandler { id: rowHoverH }
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 1
            color: root.sep
        }
        Text {
            id: lab
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            width: 140
            text: root.t(row.labelKey)
            color: root.label
            font.family: root.fonts[0]
            font.pixelSize: 13
            font.weight: Font.Medium
            elide: Text.ElideRight
        }
        Text {
            id: val
            visible: !row.pill
            anchors.left: lab.right
            anchors.leftMargin: 20
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            text: row.value
            color: root.toneColor(row.tone, false)
            font.family: row.mono ? root.mono[0] : root.fonts[0]
            font.pixelSize: row.mono ? 12 : 13
            font.weight: row.tone === "muted" ? Font.Normal : Font.Medium
            horizontalAlignment: Text.AlignRight
            wrapMode: Text.Wrap
        }
        Rectangle {
            id: pillBox
            visible: row.pill
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            height: 22
            width: Math.min(pillTxt.implicitWidth + 18, parent.width - 188)
            radius: 6
            color: root.pillBg(row.tone)
            border.width: 1
            border.color: root.pillBd(row.tone)
            Text {
                id: pillTxt
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: 9
                anchors.rightMargin: 9
                text: row.value
                color: root.toneColor(row.tone, true)
                font.family: root.fonts[0]
                font.pixelSize: 11
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
            }
        }
    }

    component FwBtn: AbstractButton {
        id: btn
        property bool primary: false
        height: 30
        implicitHeight: 30
        implicitWidth: Math.max(96, txt.implicitWidth + 32)
        hoverEnabled: true
        opacity: enabled ? 1 : 0.45
        background: Rectangle {
            radius: 8
            color: btn.primary
                   ? (btn.hovered && btn.enabled ? Qt.darker(root.blue, 1.08) : root.blue)
                   : (btn.hovered && btn.enabled ? "#2e787880" : root.fill)
            border.width: btn.activeFocus ? 1 : 0
            border.color: root.blue
        }
        contentItem: Text {
            id: txt
            text: btn.text
            color: btn.primary ? "#ffffff" : root.label
            font.family: root.fonts[0]
            font.pixelSize: 13
            font.weight: btn.primary ? Font.DemiBold : Font.Medium
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
}
