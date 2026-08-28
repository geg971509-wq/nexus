pragma ComponentBehavior: Bound

import QtQuick

Item {
    id: flow
    width: 0
    height: 0

    required property var host
    readonly property var win: host.win
    readonly property var i18: host.i18

    property bool connected: false
    property bool powerBusy: false
    property string powerOp: ""
    property string powerError: ""
    property bool tunOn: false
    property bool tunBusy: false
    property bool tunWant: false
    property bool sysOn: true
    property bool sysBusy: false
    property bool sysWant: false
    property string selectedName: "—"
    property string selectedLat: "—"
    property string connectedName: ""
    property string connectedLat: "—"
    property string activeGid: "default"
    property var catalog: null
    property bool applyingChip: false
    property var coreBaseUp: null
    property var coreBaseDown: null
    property int connPollFailStreak: 0
    property var connectedAt: null

    BackendClient {
        id: backend
        bridge: host.apiObject()
    }

    function t(k, v) { return i18 ? i18.t(k, v) : k }

    function api() { return host.apiObject() }

    function parseReply(raw) { return backend.parseReply(raw) }

    function invoke(cmd, payload) { return backend.invoke(cmd, payload) }

    function log(tag, cls, msg) {
        try {
            if (host.dockControl && typeof host.dockControl.appendLog === "function")
                host.dockControl.appendLog(tag, cls, msg)
        } catch (e) { /* host.dockControl collapsed / not ready */ }
    }

    function unwrapCatalog(blob) { return backend.unwrapCatalog(blob) }

    function nodesFromCatalog(data, gid) {
        if (!data || !data.profiles) return []
        var id = gid || data.active || "default"
        var p = data.profiles[id]
        if (!p && data.groups && data.groups.length)
            p = data.profiles[data.groups[0].id]
        if (!p) p = data.profiles.default
        var list = (p && Array.isArray(p.nodes)) ? p.nodes : []
        var out = []
        for (var i = 0; i < list.length; i++) {
            var n = list[i] || {}
            out.push(n)
        }
        return out
    }

    function looksLikeUuid(id) {
        return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(String(id || ""))
    }

    function isShareUri(s) {
        if (!s || typeof s !== "string") return false
        if (/^ss:\/\//i.test(s)) return true
        if (/^(vless|trojan|socks5?|anytls|tuic|https?):\/\//i.test(s) && s.indexOf("@") >= 0) return true
        if (/^vmess:\/\//i.test(s)) {
            var rest = s.replace(/^vmess:\/\//i, "").split("#")[0]
            if (rest.indexOf("@") < 0) return rest.length > 16
            var user = rest.split("@")[0].split(":")[0]
            return looksLikeUuid(user)
        }
        return false
    }

    function findNode(name) { return host.table.nodeByName(name) }

    function connectPayload(name) {
        var n = findNode(name)
        if (!n) return null
        if (n.outbound && n.outbound.type && n.outbound.server) {
            var ty = n.outbound.type
            if (ty === "vmess" || ty === "vless" || ty === "tuic") {
                var id = n.outbound.uuid || ""
                if (id && !looksLikeUuid(id)) return null
            }
            return { outbound: n.outbound }
        }
        if (isShareUri(n.link)) {
            var r = invoke("sub_parse_share", { body: n.link })
            var list = []
            if (r && r.ok !== false && !r.offline) {
                var d = r.data !== undefined ? r.data : r
                list = (d && d.nodes) ? d.nodes : (r.nodes || [])
            }
            if (Array.isArray(list) && list.length && list[0] && list[0].outbound
                    && list[0].outbound.type && list[0].outbound.server)
                return { outbound: list[0].outbound }
            return { link: n.link }
        }
        return null
    }

    function heroStatus() {
        var via = connectedName || selectedName
        var latShow = connected
                      ? ((connectedLat && connectedLat !== "—" && connectedLat !== "…") ? connectedLat : "—")
                      : ((selectedLat && selectedLat !== "—" && selectedLat !== "…") ? selectedLat : "—")
        var mismatch = !!(connected && connectedName && selectedName && selectedName !== "—" && selectedName !== connectedName)
        if (powerBusy) {
            var wantOn = powerOp === "connect" || powerOp === "retune" || (powerOp === "switch" && !connected)
            host.statusLabelControl.text = wantOn ? t("status.connecting") : t("status.disconnecting")
            host.statusSubControl.text = wantOn
                             ? t("status.subConnecting", { name: selectedName || via || "—" })
                             : t("status.subDisconnecting", { name: connectedName || via || "—" })
            if (win) win.sbStatus = t("sb.busy")
            return
        }
        if (powerError) {
            host.statusLabelControl.text = connected ? t("status.connected") : t("status.disconnected")
            host.statusSubControl.text = powerError
            if (win) win.sbStatus = powerError
            return
        }
        host.statusLabelControl.text = connected ? t("status.connected") : t("status.disconnected")
        if (connected && mismatch)
            host.statusSubControl.text = t("status.subMismatch", { tunnel: connectedName, selected: selectedName, lat: latShow })
        else
            host.statusSubControl.text = connected
                             ? t("status.subOn", { name: via, lat: latShow })
                             : t("status.subOff", { name: selectedName })
        if (win) win.sbStatus = connected ? t("sb.running") : t("sb.stopped")
    }

    function setConnected(on, pin) {
        connected = !!on
        if (!connected) {
            connectedName = ""
            connectedLat = "—"
            connectedAt = null
            host.dockControl.setConns([])
            connPollFailStreak = 0
            coreBaseUp = null
            coreBaseDown = null
        }
        // ponytail: no nexus.lastConnected; ON paints against current selection
        if (connected && pin !== false) {
            connectedName = selectedName
            connectedLat = (selectedLat && selectedLat !== "—" && selectedLat !== "…") ? selectedLat : "—"
            if (connectedAt == null) connectedAt = Date.now()
        }
        syncConnPoll()
        refreshSbProxy()
        heroStatus()
    }

    function syncConnPoll() {
        if (connected) {
            if (!connPoll.running) {
                coreBaseUp = null
                coreBaseDown = null
                connPoll.start()
                refreshConns()
            }
        } else {
            connPoll.stop()
        }
    }

    function loadCatalog() {
        var r = invoke("catalog_get", {})
        if (r.offline) {
            host.table.setNodes([], "")
            return
        }
        var data = unwrapCatalog(r.ok ? r.data : null)
        catalog = data
        if (!data) {
            host.table.setNodes([], "")
            return
        }
        var gid = data.active
        if (!gid && data.groups && data.groups.length) gid = data.groups[0].id
        activeGid = gid || "default"
        if (win) win.subTab = activeGid
        host.table.setNodes(nodesFromCatalog(data, activeGid), selectedName)
        refreshSbProxy()
    }

    function groupNameOf(id) {
        var data = catalog
        if (data && data.groups) {
            for (var i = 0; i < data.groups.length; i++)
                if (data.groups[i].id === id) return data.groups[i].name || id
        }
        return id || ""
    }

    function switchGroup(id, logIt) {
        if (!id) return
        var data = catalog
        if (!data) {
            var r = invoke("catalog_get", {})
            data = unwrapCatalog(r && r.ok ? (r.data || r) : (r && r.data))
        }
        if (!data) return
        var found = false
        var gname = id
        if (data.groups) {
            for (var i = 0; i < data.groups.length; i++) {
                if (data.groups[i].id === id) {
                    found = true
                    gname = data.groups[i].name || id
                    break
                }
            }
        }
        if (!found && data.groups && data.groups.length) return
        data.active = id
        catalog = data
        activeGid = id
        if (win) win.subTab = id
        invoke("catalog_put", { blob: data })
        host.table.setNodes(nodesFromCatalog(data, id), selectedName)
        refreshSbProxy()
        if (logIt !== false)
            log("SYS", "info", t("log.subSwitched", { name: gname }))
        if (win && win.settings && typeof win.settings.loadCatalog === "function")
            win.settings.loadCatalog()
    }

    function startNamed(name) {
        if (name && host.table && host.table.pickRow)
            host.table.pickRow(name)
        if (powerBusy) return
        if (connected) {
            powerError = ""
            powerBusy = true
            powerOp = "switch"
            log("SYS", "info", t("status.disconnecting"))
            heroStatus()
            kickDisconnect()
            return
        }
        togglePower()
    }

    function stopTunnel() {
        if (connected) togglePower()
        else log("SYS", "info", t("log.notConnected"))
    }

    function loadStore() {
        var r = invoke("store_snapshot", {})
        if (!r || r.offline || !r.ok) return
        var d = r.data || {}
        applyingChip = true
        host.tunControl.checked = !!d.tun
        tunOn = !!d.tun
        host.sysControl.checked = !!d.system_proxy
        sysOn = !!d.system_proxy
        applyingChip = false
    }

    function loadSession() {
        var r = invoke("session_status", {})
        var d = r && r.ok ? r.data : null
        if (d && d.running) {
            applyingChip = true
            if (d.tun != null) { host.tunControl.checked = !!d.tun; tunOn = !!d.tun }
            if (d.system_proxy != null) { host.sysControl.checked = !!d.system_proxy; sysOn = !!d.system_proxy }
            applyingChip = false
            setConnected(true, true)
        }
    }

    function failPower(msg) {
        powerError = msg || "failed"
        powerBusy = false
        powerOp = ""
        log("CORE", "warn", powerError)
        if (host.statusSubControl) host.statusSubControl.text = powerError
        if (win) win.sbStatus = powerError
    }

    function donePower() {
        powerBusy = false
        powerOp = ""
        heroStatus()
    }

    function kickConnect(args) {
        var r = invoke("connect_selected", args)
        if (!r || r.offline) { failPower("backend offline"); return false }
        if (!r.ok) { failPower(r.error || "connect failed"); return false }
        return true
    }

    function kickDisconnect() {
        var r = invoke("disconnect_selected", {})
        if (!r || r.offline) { failPower("backend offline"); return false }
        if (!r.ok) { failPower(r.error || "disconnect failed"); return false }
        return true
    }

    function togglePower() {
        if (powerBusy) return
        powerError = ""
        powerBusy = true
        powerOp = connected ? "disconnect" : "connect"
        log("SYS", "info", connected ? t("status.disconnecting") : t("status.connecting"))
        heroStatus()
        powerKick.start()
    }

    function runToggle() {
        if (powerOp === "disconnect") {
            if (!kickDisconnect()) return
            return
        }
        var name = selectedName
        if (!name || name === "—") {
            var first = host.table.firstName()
            if (first) {
                host.table.pickRow(first)
                name = host.table.selectedName || first
            }
        }
        var payload = connectPayload(name)
        if (!payload) {
            failPower(t("js.noNodePayload"))
            return
        }
        var args = {
            profile_id: 1,
            tun: tunBusy ? tunWant : tunOn,
            system_proxy: sysBusy ? sysWant : sysOn
        }
        for (var k in payload) args[k] = payload[k]
        kickConnect(args)
    }

    function onConnectResult(raw) {
        if (!powerBusy) return
        if (powerOp !== "connect" && powerOp !== "retune") return
        var r = parseReply(raw)
        var data = (r && r.data) || r || {}
        if (!r || r.offline) { failPower("backend offline"); return }
        if (!r.ok) { failPower(r.error || "connect failed"); return }
        if (data.start_error || !data.started) {
            powerError = data.start_error ? ("Start: " + data.start_error) : "start not ok"
            log("CORE", "warn", powerError)
            powerOp = "cleanup"
            kickDisconnect()
            return
        }
        if (powerOp === "retune") {
            log("OK", "ok", t("status.connected") + " · " + (connectedName || selectedName))
            donePower()
            return
        }
        setConnected(true, true)
        log("OK", "ok", t("status.connected") + " · " + (connectedName || selectedName))
        donePower()
    }

    function onDisconnectResult(raw) {
        if (!powerBusy) return
        if (powerOp !== "disconnect" && powerOp !== "switch"
                && powerOp !== "cleanup" && powerOp !== "lost")
            return
        var r = parseReply(raw)
        var next = powerOp
        setConnected(false)
        if (next === "switch") {
            if (r && !r.ok && !r.offline) {
                failPower(r.error || "disconnect failed")
                return
            }
            powerError = ""
            powerBusy = true
            powerOp = "connect"
            log("SYS", "info", t("status.connecting"))
            heroStatus()
            runToggle()
            return
        }
        if (next === "cleanup" || next === "lost") {
            powerBusy = false
            powerOp = ""
            heroStatus()
            return
        }
        if (r && !r.ok && !r.offline) {
            failPower(r.error || "disconnect failed")
            return
        }
        log("OK", "ok", t("status.disconnected"))
        donePower()
    }

    function applyTun(on) {
        if (applyingChip) return
        if (tunBusy) {
            applyingChip = true
            host.tunControl.checked = tunWant
            applyingChip = false
            return
        }
        tunWant = !!on
        tunBusy = true
        var r = invoke("set_tun_cmd", { enabled: !!on })
        if (!r || r.offline) {
            tunOn = !!on
            tunBusy = false
            return
        }
        if (!r.ok) {
            tunBusy = false
            applyingChip = true
            host.tunControl.checked = !on
            tunOn = !on
            applyingChip = false
            log("SYS", "warn", r.error || "tun failed")
            return
        }
        // kick-off: wait tun-result
    }

    function onTunResult(raw) {
        if (!tunBusy) return
        tunBusy = false
        var r = parseReply(raw)
        if (!r || r.offline || !r.ok) {
            applyingChip = true
            host.tunControl.checked = !tunWant
            tunOn = !tunWant
            applyingChip = false
            log("SYS", "warn", (r && r.error) || "tun failed")
            return
        }
        applyingChip = true
        tunOn = tunWant
        host.tunControl.checked = tunWant
        applyingChip = false
        if (r.data && r.data.note)
            log("SYS", "info", String(r.data.note))
        if (connected && !powerBusy) {
            var payload = connectPayload(connectedName || selectedName)
            if (!payload) return
            var args = { profile_id: 1, tun: tunOn, system_proxy: sysBusy ? sysWant : sysOn }
            for (var k in payload) args[k] = payload[k]
            powerError = ""
            powerBusy = true
            powerOp = "retune"
            log("SYS", "info", t("status.connecting"))
            heroStatus()
            kickConnect(args)
        }
    }

    function applySys(on) {
        if (applyingChip) return
        if (sysBusy) {
            applyingChip = true
            host.sysControl.checked = sysWant
            applyingChip = false
            return
        }
        sysWant = !!on
        sysBusy = true
        var r = invoke("set_system_proxy_cmd", { enabled: !!on })
        if (!r || r.offline) {
            sysOn = !!on
            sysBusy = false
            return
        }
        if (!r.ok) {
            sysBusy = false
            applyingChip = true
            host.sysControl.checked = !on
            sysOn = !on
            applyingChip = false
            if (r.error) log("SYS", "warn", r.error)
            return
        }
        // kick-off: wait proxy-result
    }

    function onProxyResult(raw) {
        if (!sysBusy) return
        sysBusy = false
        var r = parseReply(raw)
        if (!r || r.offline || !r.ok) {
            applyingChip = true
            host.sysControl.checked = !sysWant
            sysOn = !sysWant
            applyingChip = false
            if (r && r.error) log("SYS", "warn", r.error)
            return
        }
        applyingChip = true
        sysOn = sysWant
        host.sysControl.checked = sysWant
        applyingChip = false
        if (r.data && r.data.note)
            log("SYS", "info", String(r.data.note))
    }

    function sumAllNodeFlowBytes() {
        var total = 0
        var data = catalog
        if (data && data.profiles) {
            for (var id in data.profiles) {
                var nodes = data.profiles[id] && data.profiles[id].nodes
                if (!nodes) continue
                for (var i = 0; i < nodes.length; i++) {
                    var n = nodes[i] || {}
                    total += Math.max(0, Number(n.flowUp) || 0) + Math.max(0, Number(n.flowDown) || 0)
                }
            }
            return total
        }
        var list = host.table.raw || []
        for (var j = 0; j < list.length; j++) {
            var m = list[j] || {}
            total += Math.max(0, Number(m.flowUp) || 0) + Math.max(0, Number(m.flowDown) || 0)
        }
        return total
    }

    function refreshSbProxy() {
        if (!win) return
        var sum = sumAllNodeFlowBytes()
        win.sbProxy = (!connected && sum === 0) ? "—" : host.table.fmtBytes(sum)
    }

    function bumpCatalogFlow(name, dUp, dDown) {
        if (!catalog || !catalog.profiles || !name) return
        for (var id in catalog.profiles) {
            var nodes = catalog.profiles[id] && catalog.profiles[id].nodes
            if (!nodes) continue
            for (var i = 0; i < nodes.length; i++) {
                var n = nodes[i]
                if (!n || n.name !== name) continue
                n.flowUp = Math.max(0, Number(n.flowUp) || 0) + dUp
                n.flowDown = Math.max(0, Number(n.flowDown) || 0) + dDown
                n.flow = host.table.fmtBytes(n.flowUp) + "↑ · " + host.table.fmtBytes(n.flowDown) + "↓"
                return
            }
        }
    }

    function persistFlows() {
        var r = invoke("catalog_get", {})
        var live = unwrapCatalog(r && r.ok ? (r.data || r) : (r && r.data))
        if (!live || !live.profiles) return
        var src = catalog
        if (src && src.profiles) {
            for (var id in src.profiles) {
                var snodes = src.profiles[id] && src.profiles[id].nodes
                var lp = live.profiles[id]
                if (!snodes || !lp || !lp.nodes) continue
                for (var i = 0; i < snodes.length; i++) {
                    var s = snodes[i]
                    if (!s || !s.name) continue
                    for (var j = 0; j < lp.nodes.length; j++) {
                        var n = lp.nodes[j]
                        if (!n || n.name !== s.name) continue
                        n.flowUp = Math.max(0, Number(s.flowUp) || 0)
                        n.flowDown = Math.max(0, Number(s.flowDown) || 0)
                        if (s.flow) n.flow = s.flow
                        break
                    }
                }
            }
        }
        invoke("catalog_put", { blob: live })
        catalog = live
    }

    function applyNodeFlow(up, down) {
        up = Math.max(0, Number(up) || 0)
        down = Math.max(0, Number(down) || 0)
        if (coreBaseUp == null || coreBaseDown == null) {
            coreBaseUp = up
            coreBaseDown = down
            refreshSbProxy()
            return
        }
        if (up < coreBaseUp || down < coreBaseDown) {
            coreBaseUp = up
            coreBaseDown = down
            refreshSbProxy()
            return
        }
        var dUp = up - coreBaseUp
        var dDown = down - coreBaseDown
        coreBaseUp = up
        coreBaseDown = down
        if (dUp === 0 && dDown === 0) {
            refreshSbProxy()
            return
        }
        var name = connectedName || ""
        if (name) {
            host.table.addFlow(name, dUp, dDown)
            bumpCatalogFlow(name, dUp, dDown)
            catalogSave.restart()
        }
        refreshSbProxy()
    }

    function refreshConns() {
        if (!connected) {
            host.dockControl.setConns([])
            connPollFailStreak = 0
            return
        }
        var pollOk = false
        var r = invoke("query_connections", {})
        if (r && r.ok && r.data) {
            pollOk = true
            if (host.dockControl.open && host.dockControl.panel === "conn")
                host.dockControl.setConns(r.data.active || r.data.connections || r.data)
        }
        var st = invoke("query_stats", {})
        if (st && st.ok && st.data) {
            applyNodeFlow(st.data.upload, st.data.download)
            pollOk = true
        }
        if (pollOk) {
            connPollFailStreak = 0
            return
        }
        connPollFailStreak += 1
        if (connPollFailStreak < 3 || !connected || powerBusy) return
        connPollFailStreak = 0
        log("SYS", "warn", t("log.coreLost"))
        powerError = ""
        powerBusy = true
        powerOp = "lost"
        heroStatus()
        kickDisconnect()
    }

    Timer {
        id: connPoll
        interval: 1500
        repeat: true
        onTriggered: flow.refreshConns()
    }

    Timer {
        id: catalogSave
        interval: 80
        repeat: false
        onTriggered: flow.persistFlows()
    }

    Timer {
        id: powerKick
        interval: 50
        repeat: false
        onTriggered: flow.runToggle()
    }

    Connections {
        target: flow.api()
        function onEvent(name, json) {
            if (name === "connect-result") {
                flow.onConnectResult(json)
                return
            }
            if (name === "disconnect-result") {
                flow.onDisconnectResult(json)
                return
            }
            if (name === "tun-result") {
                flow.onTunResult(json)
                return
            }
            if (name === "proxy-result")
                flow.onProxyResult(json)
        }
    }

    function initialize() {
        heroStatus()
        if (!api()) return
        loadCatalog()
        loadStore()
        loadSession()
    }

}
