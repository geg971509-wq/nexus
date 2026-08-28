import QtQuick

QtObject {
    id: flow

    required property var host
    readonly property var win: host.win
    readonly property var home: host.home

    property string eOrigName: ""
    property string eName: ""
    property string eType: "VLESS"
    property string eServer: ""
    property string ePort: "443"
    property string eUuid: ""
    property string eFlow: ""
    property string eSecurity: "auto"
    property string eAlterId: "0"
    property string eUser: ""
    property string ePass: ""
    property string eMethod: "aes-128-gcm"
    property string eSni: ""
    property string eNetwork: ""
    property string eHost: ""
    property string ePath: ""
    property string eCongest: ""
    property string eAlpn: ""
    property string eNote: ""
    property bool eTls: false
    property bool eInsecure: false
    property string qrName: ""
    property string qrLink: ""
    property string qrSvg: ""
    property int resolveGot: 0
    property int resolveExpect: 0
    readonly property string eKey: editTypeKey(eType)

    function t(k, v) { return host.t(k, v) }
    function api() { return host.api() }
    function invoke(cmd, payload) { return host.invoke(cmd, payload) }
    function parsedOf(r) { return host.parsedOf(r) }
    function parseAddr(addr) { return host.parseAddr(addr) }
    function loadCatalogBlob() { return host.loadCatalogBlob() }
    function activeGroup() { return host.activeGroup() }
    function putCatalog(blob) { return host.putCatalog(blob) }
    function reloadHome() { host.reloadHome() }
    function log(tag, cls, msg) { host.log(tag, cls, msg) }
    function askConfirm(msg, opts, cb) { host.askConfirm(msg, opts, cb) }

    function clipSet(text) {
        var a = api()
        if (!a || typeof a.setClipboardText !== "function") return false
        a.setClipboardText(String(text || ""))
        return true
    }

    function looksLikeUuid(id) {
        return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(String(id || ""))
            || /^[0-9a-f]{32}$/i.test(String(id || ""))
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

    function nodeShareLink(n) {
        if (!n) return ""
        if (n.link && isShareUri(n.link)) return n.link
        if (n.outbound && typeof n.outbound === "object") {
            try { return JSON.stringify(n.outbound) } catch (e) {}
        }
        return ""
    }

    function tableSel() {
        var table = home && home.table
        if (!table) return []
        if (typeof table.selectedNodeList === "function")
            return table.selectedNodeList()
        var n = table.nodeByName ? table.nodeByName(table.selectedName) : null
        return n ? [n] : []
    }

    function liveProfile(data) {
        if (!data) data = loadCatalogBlob()
        if (!data) return null
        if (!data.profiles) data.profiles = {}
        var g = activeGroup()
        var id = (g && g.id) || "default"
        var prof = data.profiles[id]
        if (!prof) {
            prof = { label: (g && g.name) || id, nodes: [] }
            data.profiles[id] = prof
        }
        if (!Array.isArray(prof.nodes)) prof.nodes = []
        return { data: data, g: g, id: id, prof: prof }
    }

    function putLive(pack) {
        if (!pack || !pack.data) return
        if (pack.g) pack.g.count = pack.prof.nodes.length
        putCatalog(pack.data)
        reloadHome()
    }

    function normalizeType(ty) {
        var u = String(ty || "VLESS").toUpperCase().replace(/[^A-Z0-9]/g, "")
        if (u === "VLESS") return "VLESS"
        if (u === "VMESS") return "VMess"
        if (u === "TROJAN") return "Trojan"
        if (u === "SS" || u === "SHADOWSOCKS") return "SS"
        if (u === "HTTPS") return "HTTPS"
        if (u === "HTTP") return "HTTP"
        if (u === "SOCKS" || u === "SOCKS5") return "SOCKS"
        if (u === "ANYTLS") return "AnyTLS"
        if (u === "TUIC" || u === "TUIC5") return "TUIC"
        return ty || "VLESS"
    }

    function editTypeKey(typeLabel) {
        var u = String(typeLabel || "VLESS").toUpperCase().replace(/[^A-Z0-9]/g, "")
        if (u === "VLESS") return "vless"
        if (u === "VMESS") return "vmess"
        if (u === "TROJAN") return "trojan"
        if (u === "SS" || u === "SHADOWSOCKS") return "ss"
        if (u === "HTTPS") return "https"
        if (u === "HTTP") return "http"
        if (u === "SOCKS" || u === "SOCKS5") return "socks"
        if (u === "ANYTLS") return "anytls"
        if (u === "TUIC" || u === "TUIC5") return "tuic"
        return u.toLowerCase()
    }

    function eShow(keys) {
        return (" " + keys + " ").indexOf(" " + eKey + " ") >= 0
    }

    function fieldsFromOutbound(ob, fallback) {
        var o = (ob && typeof ob === "object") ? ob : {}
        var ty = String(o.type || (fallback && fallback.type) || "vless").toLowerCase()
        var tls = (o.tls && typeof o.tls === "object") ? o.tls : {}
        var displayType = (fallback && fallback.type) || ty
        if (ty === "http" && (tls.enabled === true || (fallback && fallback.type === "HTTPS")))
            displayType = "HTTPS"
        else if (ty === "http")
            displayType = (fallback && fallback.type === "HTTP") ? "HTTP" : (tls.enabled ? "HTTPS" : "HTTP")
        else if (ty === "anytls") displayType = "AnyTLS"
        else if (ty === "socks") displayType = "SOCKS"
        else if (ty === "vmess") displayType = "VMess"
        else if (ty === "tuic") displayType = "TUIC"
        var server = o.server || String((fallback && fallback.addr) || "").split(":")[0] || ""
        var port = o.server_port || o.port || String((fallback && fallback.addr) || "").split(":")[1] || "443"
        var tr = (o.transport && typeof o.transport === "object") ? o.transport : {}
        var hostHdr = ""
        if (tr.headers && typeof tr.headers === "object")
            hostHdr = tr.headers.Host || tr.headers.host || ""
        if (!hostHdr && tr.host) hostHdr = String(tr.host)
        var alpn = ""
        if (Array.isArray(tls.alpn)) alpn = tls.alpn.join(",")
        else if (typeof tls.alpn === "string") alpn = tls.alpn
        return {
            name: (fallback && fallback.name) || "",
            type: normalizeType(displayType),
            server: server,
            port: String(port),
            uuid: o.uuid || "",
            flow: o.flow || "",
            security: o.security || "auto",
            alterId: o.alter_id != null ? o.alter_id : (o.alterId != null ? o.alterId : 0),
            username: o.username || "",
            password: o.password || "",
            method: o.method || o.cipher || "",
            sni: (tls.server_name || o.sni || "") || "",
            network: tr.type || o.network || "",
            path: tr.path || o.path || "",
            host: hostHdr || "",
            insecure: !!(tls.insecure || o.insecure),
            tls: tls.enabled === true,
            congestion_control: o.congestion_control || "",
            alpn: alpn,
            note: (fallback && fallback.note) || ""
        }
    }

    function buildOutboundFromFields(f) {
        var typeRaw = String(f.type || "").toLowerCase()
        var server = String(f.server || "").trim()
        var port = Number(f.port || 443) || 443
        if (!server) return null
        var ty = typeRaw
        if (ty === "ss" || ty === "shadowsocks") ty = "shadowsocks"
        else if (ty === "socks" || ty === "socks5") ty = "socks"
        else if (ty === "http" || ty === "https") ty = "http"
        else if (ty === "vmess") ty = "vmess"
        else if (ty === "vless") ty = "vless"
        else if (ty === "trojan") ty = "trojan"
        else if (ty === "anytls") ty = "anytls"
        else if (ty === "tuic" || ty === "tuic5") ty = "tuic"
        if (!ty) return null
        var ob = { type: ty, tag: "proxy", server: server, server_port: port }
        var user = f.username || f.user || ""
        var pass = f.password || f.passwd || ""
        var uuid = f.uuid || f.id || ""
        if (ty === "http" || ty === "socks") {
            if (user) ob.username = user
            if (pass) ob.password = pass
            if (ty === "http") {
                if (typeof f.path === "string" && f.path) ob.path = f.path
                var wantTls = f.tls === true || typeRaw === "https" || !!f.sni
                if (wantTls) {
                    ob.tls = { enabled: true }
                    if (f.sni) ob.tls.server_name = f.sni
                    if (f.skip) ob.tls.insecure = true
                }
            }
        } else if (ty === "shadowsocks") {
            if (!pass) return null
            ob.method = f.cipher || f.method || "aes-128-gcm"
            ob.password = pass
        } else if (ty === "vmess") {
            if (!uuid) return null
            ob.uuid = uuid
            var sec = f.cipher || f.security || f.method || "auto"
            if (sec && sec !== "auto" && sec !== "none" && sec !== "tls") ob.security = sec
            var aid = Number(f.alterId != null ? f.alterId : (f.alter_id != null ? f.alter_id : 0)) || 0
            if (aid > 0) ob.alter_id = aid
            if (f.tls === true || f.tls === "tls" || f.tls === "true") {
                ob.tls = { enabled: true }
                if (f.sni) ob.tls.server_name = f.sni
                if (f.skip) ob.tls.insecure = true
            }
            var net = String(f.network || f.net || "tcp").toLowerCase()
            if (net && net !== "tcp" && net !== "raw" && net !== "none") {
                var tr = { type: (net === "websocket" || net === "ws") ? "ws" : net }
                if (f.path) tr.path = f.path
                if (f.host) tr.headers = { Host: f.host }
                ob.transport = tr
            }
        } else if (ty === "vless") {
            if (!uuid) return null
            ob.uuid = uuid
            if (f.flow) ob.flow = f.flow
            if (f.tls !== false) {
                ob.tls = { enabled: true }
                if (f.sni) ob.tls.server_name = f.sni
                if (f.skip) ob.tls.insecure = true
            }
            var net2 = String(f.network || f.net || "tcp").toLowerCase()
            if (net2 && net2 !== "tcp" && net2 !== "raw" && net2 !== "none") {
                var tr2 = { type: (net2 === "websocket" || net2 === "ws") ? "ws" : net2 }
                if (f.path) tr2.path = f.path
                if (f.host) tr2.headers = { Host: f.host }
                ob.transport = tr2
            }
        } else if (ty === "trojan" || ty === "anytls") {
            if (!pass) return null
            ob.password = pass
            ob.tls = { enabled: true }
            if (f.sni) ob.tls.server_name = f.sni
            if (f.skip) ob.tls.insecure = true
        } else if (ty === "tuic") {
            if (!uuid) return null
            ob.uuid = uuid
            if (pass) ob.password = pass
            var cc = String(f.congestion_control || f.congestion || "").trim()
            if (cc && cc !== "none") ob.congestion_control = cc.toLowerCase()
            ob.tls = { enabled: true }
            if (f.sni) ob.tls.server_name = f.sni
            if (f.skip) ob.tls.insecure = true
            if (f.alpn) {
                var list = String(f.alpn).split(/[,\s]+/).filter(function (s) { return !!s })
                if (list.length) ob.tls.alpn = list
            }
        } else {
            return null
        }
        return ob
    }

    function hydrateNode(n) {
        if (!n || n.outbound || !n.link) return n
        var r = invoke("sub_parse_share", { body: n.link })
        var nodes = parsedOf(r).nodes
        if (nodes.length && nodes[0].outbound) {
            n.outbound = nodes[0].outbound
            if (nodes[0].addr) n.addr = nodes[0].addr
        }
        return n
    }

    function openEdit(name) {
        var table = home && home.table
        var n = table && table.nodeByName ? table.nodeByName(name) : null
        if (!n) {
            var sel = tableSel()
            n = sel[0]
        }
        if (!n) { log("SYS", "warn", t("log.noNode")); return }
        hydrateNode(n)
        var f = fieldsFromOutbound(n.outbound, { name: n.name, type: n.type, addr: n.addr, note: n.note })
        if (!n.outbound && n.addr) {
            var bits = String(n.addr).split(":")
            if (bits[0]) f.server = bits[0]
            if (bits[1]) f.port = bits[1]
        }
        eOrigName = n.name || ""
        eName = f.name
        eType = f.type || "VLESS"
        eServer = f.server
        ePort = f.port
        eUuid = f.uuid
        eFlow = f.flow
        eSecurity = f.security || "auto"
        eAlterId = String(f.alterId != null ? f.alterId : 0)
        eUser = f.username
        ePass = f.password
        eMethod = f.method || "aes-128-gcm"
        eSni = f.sni
        eNetwork = f.network
        eHost = f.host
        ePath = f.path
        eTls = !!f.tls
        eInsecure = !!f.insecure
        eCongest = f.congestion_control || "bbr"
        eAlpn = f.alpn || (editTypeKey(f.type) === "tuic" ? "h3" : "")
        eNote = f.note || ""
        var types = ["VLESS", "Trojan", "SS", "VMess", "HTTP", "HTTPS", "SOCKS", "AnyTLS", "TUIC"]
        var ti = types.indexOf(eType)
        host.selectEditType(ti < 0 ? 0 : ti)
        host.showEditDialog()
    }

    function saveEdit() {
        var name = (eName || "").trim() || t("js.unnamed")
        var host = (eServer || "").trim() || "0.0.0.0"
        var port = (ePort || "").trim() || "443"
        var type = eType || "VLESS"
        var tk = editTypeKey(type)
        var fields = {
            type: type,
            server: host,
            port: port,
            uuid: eUuid,
            flow: eFlow,
            security: eSecurity,
            cipher: eSecurity || eMethod,
            method: eMethod,
            alterId: eAlterId || 0,
            username: eUser,
            password: ePass,
            sni: eSni,
            network: eNetwork,
            path: ePath,
            host: eHost,
            skip: eInsecure,
            congestion_control: eCongest,
            alpn: eAlpn,
            tls: tk === "https" || tk === "trojan" || tk === "tuic" || tk === "anytls"
        }
        if (tk === "ss") { fields.cipher = eMethod; fields.method = eMethod }
        if (tk === "http") fields.tls = !!eTls || !!eSni
        if (tk === "https") { fields.tls = true; fields.type = "https" }
        if (tk === "anytls") { fields.tls = true; fields.type = "anytls" }
        if (tk === "tuic") { fields.tls = true; fields.type = "tuic" }
        if (tk === "vmess") { fields.tls = !!eTls; fields.type = "vmess"; fields.cipher = eSecurity || "auto"; fields.security = fields.cipher }
        if (tk === "vless") { fields.tls = !!eTls; fields.type = "vless" }
        if ((tk === "http" || tk === "https" || tk === "socks") && !fields.username && fields.password)
            fields.username = fields.password
        var outbound = buildOutboundFromFields(fields)
        var pack = liveProfile()
        if (!pack) return
        var oldName = eOrigName
        var node = null
        for (var i = 0; i < pack.prof.nodes.length; i++) {
            if (pack.prof.nodes[i] && pack.prof.nodes[i].name === oldName) {
                node = pack.prof.nodes[i]
                break
            }
        }
        if (!node) { log("SYS", "warn", t("log.noNode")); return }
        node.name = name
        node.type = type
        node.addr = host + ":" + port
        if (eNote) node.note = eNote
        else delete node.note
        if (outbound) {
            if (node.outbound && typeof node.outbound === "object") {
                var prev = node.outbound
                var keep = JSON.parse(JSON.stringify(outbound))
                var extras = ["multiplex", "dialer", "detour", "domain_strategy", "domain_resolver", "bind_interface", "routing_mark", "packet_encoding", "global_padding", "authenticated_length"]
                for (var k = 0; k < extras.length; k++)
                    if (prev[extras[k]] != null && keep[extras[k]] == null) keep[extras[k]] = prev[extras[k]]
                if (prev.tls && typeof prev.tls === "object" && keep.tls && typeof keep.tls === "object") {
                    var tlsKeep = ["utls", "reality", "alpn", "min_version", "max_version"]
                    for (var ti = 0; ti < tlsKeep.length; ti++)
                        if (prev.tls[tlsKeep[ti]] != null && keep.tls[tlsKeep[ti]] == null)
                            keep.tls[tlsKeep[ti]] = prev.tls[tlsKeep[ti]]
                }
                node.outbound = keep
            } else {
                node.outbound = outbound
            }
            if (node.link) delete node.link
        }
        if (home && home.connected && home.connectedName === oldName)
            home.connectedName = name
        putLive(pack)
        if (home && home.table && home.table.pickRow)
            home.table.pickRow(name)
        host.hideEditDialog()
        log("SYS", "ok", t("log.nodeSaved", { name: name }))
    }

    function openQr(name) {
        var table = home && home.table
        var n = table && table.nodeByName ? table.nodeByName(name) : null
        if (!n) {
            var sel = tableSel()
            n = sel[0]
        }
        qrName = (n && n.name) || name || ""
        qrLink = nodeShareLink(n)
        qrSvg = ""
        host.showQrDialog()
        if (!qrLink) return
        var r = invoke("qr_svg", { text: qrLink })
        var d = r && r.ok ? (r.data || r) : r
        if (d && d.svg) qrSvg = String(d.svg)
        else log("SYS", "warn", t("log.qrFail", { error: (r && r.error) || "qr" }))
    }

    function copyQr() {
        if (!qrLink) { log("SYS", "warn", t("log.noLinkCopy")); return }
        if (clipSet(qrLink)) log("SYS", "ok", t("log.linkCopied"))
    }

    function deleteSelected() {
        var sel = tableSel()
        if (!sel.length) { log("SYS", "warn", t("log.noNode")); return }
        if (sel.length > 1) {
            askConfirm(t("confirm.deleteNodes", { n: sel.length }), {
                title: t("confirm.deleteNodesTitle"),
                okText: t("ctx.delete"),
                danger: true
            }, function (ok) { if (ok) dropNames(sel) })
            return
        }
        dropNames(sel)
    }

    function dropNames(sel) {
        var names = {}
        for (var i = 0; i < sel.length; i++)
            if (sel[i] && sel[i].name) names[sel[i].name] = true
        var pack = liveProfile()
        if (!pack) return
        var next = []
        for (var j = 0; j < pack.prof.nodes.length; j++) {
            var n = pack.prof.nodes[j]
            if (!n || names[n.name]) continue
            next.push(n)
        }
        pack.prof.nodes = next
        putLive(pack)
        log("SYS", "warn", t("log.deletedN", { n: sel.length }))
    }

    function cloneSelected() {
        var sel = tableSel()
        if (!sel.length) { log("SYS", "warn", t("log.noNode")); return }
        var src = sel[0]
        var pack = liveProfile()
        if (!pack) return
        var name = t("js.nodeCopy", { name: src.name || t("js.nodes") })
        var copy = {
            name: name,
            type: src.type,
            addr: src.addr,
            lat: null,
            flow: null,
            flowUp: 0,
            flowDown: 0,
            link: src.link || "",
            outbound: src.outbound ? JSON.parse(JSON.stringify(src.outbound)) : null
        }
        var idx = -1
        for (var i = 0; i < pack.prof.nodes.length; i++)
            if (pack.prof.nodes[i] && pack.prof.nodes[i].name === src.name) idx = i
        if (idx >= 0) pack.prof.nodes.splice(idx + 1, 0, copy)
        else pack.prof.nodes.push(copy)
        putLive(pack)
        log("SYS", "ok", t("log.cloned", { name: name }))
    }

    function copyLinkSelected() {
        var sel = tableSel()
        if (!sel.length) { log("SYS", "warn", t("log.noNode")); return }
        var link = nodeShareLink(sel[0])
        if (!link) { log("SYS", "warn", t("log.noLinkCopy")); return }
        if (clipSet(link)) log("SYS", "ok", t("log.copiedLinkNamed", { name: sel[0].name }))
    }

    function resetTrafficSelected() {
        var table = home && home.table
        if (!table) return
        var sel = tableSel()
        var targets = sel.length ? sel : (table.raw || [])
        var names = {}
        for (var i = 0; i < targets.length; i++)
            if (targets[i] && targets[i].name) names[targets[i].name] = true
        var pack = liveProfile()
        if (pack) {
            for (var id in pack.data.profiles) {
                var nodes = pack.data.profiles[id] && pack.data.profiles[id].nodes
                if (!nodes) continue
                for (var j = 0; j < nodes.length; j++) {
                    var n = nodes[j]
                    if (!n || !names[n.name]) continue
                    n.flow = null
                    n.flowUp = 0
                    n.flowDown = 0
                }
            }
            putLive(pack)
        }
        if (home) {
            home.coreBaseUp = null
            home.coreBaseDown = null
            if (typeof home.refreshSbProxy === "function") home.refreshSbProxy()
        }
        log("SYS", "info", t("log.resetTraffic", { n: targets.length }))
    }

    function dedupeNodes() {
        var pack = liveProfile()
        if (!pack) return
        var seen = {}
        var next = []
        var dropped = 0
        for (var i = 0; i < pack.prof.nodes.length; i++) {
            var n = pack.prof.nodes[i]
            if (!n) continue
            var key = (n.addr || "") + "|" + (n.type || "")
            if (seen[key]) { dropped++; continue }
            seen[key] = true
            next.push(n)
        }
        if (!dropped) { log("SYS", "info", t("log.noDupes")); return }
        pack.prof.nodes = next
        putLive(pack)
        log("SYS", "ok", t("log.deduped", { n: dropped }))
    }

    function latFailed(n) {
        var s = n && n.lat
        if (s == null || s === "" || s === "—" || s === "…") return false
        if (typeof s === "number") return !isFinite(s) || s < 0
        var t = String(s)
        if (/timeout|fail|error|不可用|aborted/i.test(t)) return true
        return false
    }

    function latEmpty(n) {
        var s = n && n.lat
        if (s == null || s === "" || s === "—" || s === "…") return true
        if (typeof s === "number") return !isFinite(s) || s < 0
        return /timeout|fail|error/i.test(String(s))
    }

    function removeByPred(pred, label) {
        var pack = liveProfile()
        if (!pack) return
        var next = []
        var dropped = 0
        for (var i = 0; i < pack.prof.nodes.length; i++) {
            var n = pack.prof.nodes[i]
            if (!n) continue
            if (pred(n)) { dropped++; continue }
            next.push(n)
        }
        if (!dropped) {
            log("SYS", "info", t("log.removed0", { label: label || t("log.remove") }))
            return
        }
        pack.prof.nodes = next
        putLive(pack)
        log("SYS", "ok", t("log.removedN", { label: label || t("log.remove"), n: dropped }))
    }

    function resolveSelected() {
        if (resolveExpect > 0 && resolveGot < resolveExpect) {
            log("TEST", "warn", t("log.exitIpBusy"))
            return
        }
        var sel = tableSel()
        if (!sel.length) { log("SYS", "warn", t("log.noNode")); return }
        var targets = []
        for (var i = 0; i < sel.length; i++) {
            var p = parseAddr(sel[i].addr)
            if (!p) continue
            targets.push({ id: sel[i].name, host: p.host })
        }
        if (!targets.length) { log("SYS", "warn", t("log.noValidAddr")); return }
        resolveExpect = targets.length
        resolveGot = 0
        var started = invoke("net_resolve_hosts", { targets: targets, concurrency: 16 })
        if (!started || started.ok === false) {
            resolveExpect = 0
            log("SYS", "warn", t("log.exitIpFail", { id: "batch", error: (started && started.error) || "resolve" }))
            return
        }
        log("TEST", "info", t("log.exitIpStart", { n: targets.length }))
    }

    function applyResolved(id, ip) {
        if (!id || !ip) return
        var pack = liveProfile()
        if (!pack) return
        for (var i = 0; i < pack.prof.nodes.length; i++) {
            var n = pack.prof.nodes[i]
            if (!n || n.name !== id) continue
            var p = parseAddr(n.addr)
            var port = p ? p.port : 443
            n.addr = ip + ":" + port
            putLive(pack)
            return
        }
    }

}
