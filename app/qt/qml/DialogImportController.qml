import QtQuick

QtObject {
    id: flow

    required property var host
    readonly property var win: host.win
    readonly property var home: host.home
    readonly property var catalog: host.catalog

    property bool subUpdating: false
    property var subJob: null
    property var subQueue: []

    function t(k, v) { return host.t(k, v) }
    function api() { return host.api() }
    function invoke(cmd, payload) { return host.invoke(cmd, payload) }
    function parseReply(raw) { return host.parseReply(raw) }
    function loadCatalogBlob() { return host.loadCatalogBlob() }
    function activeGroup() { return host.activeGroup() }
    function putCatalog(blob) { return host.putCatalog(blob) }
    function reloadHome() { host.reloadHome() }
    function log(tag, cls, msg) { host.log(tag, cls, msg) }

    function parsedOf(r) {
        if (!r || r.ok === false || r.offline) return { nodes: [], skipped: [] }
        var d = r.data !== undefined ? r.data : r
        var list = (d && d.nodes) ? d.nodes : (r.nodes || [])
        var skipped = (d && d.skipped) ? d.skipped : (r.skipped || [])
        return {
            nodes: Array.isArray(list) ? list : [],
            skipped: Array.isArray(skipped) ? skipped : []
        }
    }

    function parseBody(body) {
        var text = String(body || "").trim()
        if (!text) return { nodes: [], skipped: [] }
        if (/^<!DOCTYPE\s+html/i.test(text) || /^<html[\s>]/i.test(text))
            return { nodes: [], skipped: [] }
        var parsed = parsedOf(invoke("sub_parse_share", { body: text }))
        if (parsed.nodes.length) return parsed
        try {
            var dec = Qt.atob(String(text).replace(/\s/g, ""))
            if (dec && dec !== text) {
                parsed = parsedOf(invoke("sub_parse_share", { body: dec }))
                if (parsed.nodes.length) return parsed
                text = String(dec).trim()
            }
        } catch (e) { /* not b64 */ }
        if (/proxies\s*:/i.test(text))
            parsed = parsedOf(invoke("sub_parse_clash", { body: text }))
        return parsed
    }

    function logSkipped(parsed) {
        var skipped = parsed && Array.isArray(parsed.skipped) ? parsed.skipped : []
        if (skipped.length)
            log("SYS", "warn", t("log.importSkipped", { list: skipped.join(", ") }))
    }

    function addNodes(parsed, src) {
        var nodes = parsed && Array.isArray(parsed.nodes) ? parsed.nodes : []
        logSkipped(parsed)
        if (!nodes.length) {
            log("SYS", "warn", t("log.importNone", { src: src || t("log.importSrc") }))
            return
        }
        var data = loadCatalogBlob()
        if (!data) return
        if (!data.profiles) data.profiles = {}
        var g = activeGroup()
        var id = g.id || "default"
        var prof = data.profiles[id] || { label: g.name, nodes: [] }
        if (!Array.isArray(prof.nodes)) prof.nodes = []
        var seen = {}
        for (var i = 0; i < prof.nodes.length; i++)
            seen[(prof.nodes[i].addr || "") + "|" + (prof.nodes[i].type || "")] = true
        var added = 0
        for (var j = 0; j < nodes.length; j++) {
            var n = nodes[j] || {}
            var key = (n.addr || "") + "|" + (n.type || "")
            if (seen[key]) continue
            seen[key] = true
            prof.nodes.push(n)
            added++
        }
        data.profiles[id] = prof
        g.count = prof.nodes.length
        putCatalog(data)
        reloadHome()
        var extra = added < nodes.length ? t("log.importDedupe", { n: nodes.length - added }) : ""
        log("SYS", "ok", t("log.importedNExtra", { n: added, dedupe: extra, src: src || "" }))
    }

    function applyUrl(url, srcKey) {
        var data = loadCatalogBlob()
        if (!data || !data.groups) return
        var g = activeGroup()
        g.url = url
        putCatalog(data)
        log("SYS", "info", t(srcKey))
        if (subUpdating) {
            log("SYS", "warn", t("log.subBusy"))
            return
        }
        kickFetch(g)
    }

    function isSubscriptionUrl(text) {
        var value = String(text || "").trim()
        return /^https?:\/\//i.test(value)
                && value.indexOf("\n") < 0
                && value.indexOf("@") < 0
                && value.length < 500
    }

    function kickFetch(g) {
        if (!g || !g.url) return false
        log("SYS", "info", t("log.subUpdating", { name: g.name || g.id }))
        var data = catalog || loadCatalogBlob()
        if (!data) return false
        if (!data.profiles) data.profiles = {}
        var prof = data.profiles[g.id] || { label: g.name, nodes: [] }
        var before = Array.isArray(prof.nodes) ? prof.nodes : []
        subJob = { id: g.id, name: g.name || g.id, url: g.url, keepN: before.length }
        subUpdating = true
        var r = invoke("sub_fetch", { url: g.url })
        if (!r || r.offline || !r.ok) {
            subJob = null
            if (!(subQueue && subQueue.length)) subUpdating = false
            log("SYS", "warn", t("log.subFailKeep", {
                name: g.name || g.id,
                error: (r && r.error) || "fetch",
                n: before.length
            }))
            kickNextSub()
            return false
        }
        return true
    }

    function kickNextSub() {
        var q = subQueue || []
        if (!q.length) {
            subQueue = []
            subUpdating = !!subJob
            return
        }
        var n = q[0]
        var rest = []
        for (var i = 1; i < q.length; i++) rest.push(q[i])
        subQueue = rest
        kickFetch(n)
    }

    function applyFetchedBody(job, fetched) {
        if (!job) return false
        if (!fetched || fetched.offline || fetched.ok === false) {
            log("SYS", "warn", t("log.subFailKeep", {
                name: job.name,
                error: (fetched && fetched.error) || "fetch",
                n: job.keepN
            }))
            return false
        }
        var body = (fetched.data && fetched.data.body !== undefined) ? fetched.data.body : fetched.body
        if (typeof body !== "string") {
            log("SYS", "warn", t("log.subFailKeep", { name: job.name, error: "empty", n: job.keepN }))
            return false
        }
        var parsed = parseBody(body)
        var next = parsed.nodes
        logSkipped(parsed)
        if (!next.length) {
            log("SYS", "warn", t("log.subFailKeep", {
                name: job.name,
                error: t("js.subEmptyParse"),
                n: job.keepN
            }))
            return false
        }
        var data = loadCatalogBlob()
        if (!data) return false
        if (!data.profiles) data.profiles = {}
        var g = null
        if (data.groups) {
            for (var i = 0; i < data.groups.length; i++)
                if (data.groups[i].id === job.id) { g = data.groups[i]; break }
        }
        if (!g) g = { id: job.id, name: job.name, url: job.url }
        var prof = data.profiles[g.id] || { label: g.name, nodes: [] }
        var before = Array.isArray(prof.nodes) ? prof.nodes : []
        var byName = {}, byAddr = {}
        var j
        for (j = 0; j < before.length; j++) {
            var old = before[j]
            if (old && old.name) byName[old.name] = old
            if (old && old.addr) byAddr[old.addr] = old
        }
        var beforeKeys = {}, nextKeys = {}, addR = 0, delR = 0
        for (j = 0; j < before.length; j++)
            beforeKeys[(before[j].addr || "") + "|" + (before[j].name || "")] = true
        for (j = 0; j < next.length; j++) {
            var n = next[j]
            var nk = (n.addr || "") + "|" + (n.name || "")
            nextKeys[nk] = true
            if (!beforeKeys[nk]) addR++
            var prev = byName[n.name] || byAddr[n.addr]
            if (!prev) continue
            if (prev.flowUp || prev.flowDown || prev.flow) {
                n.flowUp = Math.max(0, Number(prev.flowUp) || 0)
                n.flowDown = Math.max(0, Number(prev.flowDown) || 0)
                n.flow = prev.flow || null
            }
            if (prev.lat != null && n.lat == null) n.lat = prev.lat
        }
        for (var k in beforeKeys)
            if (!nextKeys[k]) delR++
        prof.nodes = next
        prof.label = g.name || prof.label
        data.profiles[g.id] = prof
        g.count = next.length
        putCatalog(data)
        reloadHome()
        if (win && win.settings && typeof win.settings.loadCatalog === "function")
            win.settings.loadCatalog()
        log("OK", "ok", t("log.subUpdated", {
            name: g.name || g.id,
            add: addR,
            del: delR,
            total: next.length
        }))
        return true
    }

    function onSubFetchResult(raw) {
        var job = subJob
        if (!job) return
        subJob = null
        applyFetchedBody(job, parseReply(raw))
        kickNextSub()
    }

    function readLocal(url) {
        var xhr = new XMLHttpRequest()
        xhr.open("GET", url, false)
        xhr.send()
        return xhr.responseText || ""
    }

    function importClip() {
        var a = api()
        if (!a || typeof a.clipboardText !== "function") {
            log("SYS", "warn", t("log.clipFail"))
            return
        }
        var text = String(a.clipboardText() || "").trim()
        if (!text) { log("SYS", "warn", t("log.clipEmpty")); return }
        if (isSubscriptionUrl(text)) {
            applyUrl(text, "log.clipIsSub")
            return
        }
        addNodes(parseBody(text), t("js.clipboard"))
    }

    function importQrFile(url) {
        var a = api()
        if (!a || typeof a.decodeQrFile !== "function") {
            log("SYS", "warn", t("log.qrReadFail", { error: "decoder unavailable" }))
            return
        }
        var r
        try {
            r = parseReply(a.decodeQrFile(String(url)))
        } catch (e) {
            r = { ok: false, error: String(e) }
        }
        if (!r || r.ok === false) {
            log("SYS", "warn", t("log.qrReadFail", { error: (r && r.error) || "decode" }))
            return
        }
        var d = r.data || r
        var values = d && Array.isArray(d.values) ? d.values : []
        if (!values.length) {
            log("SYS", "warn", t("log.qrNone"))
            return
        }
        var text = values.join("\n").trim()
        if (values.length === 1 && isSubscriptionUrl(text)) {
            applyUrl(text, "log.qrIsSub")
            return
        }
        var name = String(url).split("/").pop()
        try { name = decodeURIComponent(name) } catch (e) { /* keep encoded filename */ }
        addNodes(parseBody(text), name)
    }

    function importFileUrl(url) {
        try {
            var text = readLocal(url)
            var name = String(url).split("/").pop()
            var trimmed = String(text || "").trim()
            if (isSubscriptionUrl(trimmed)) {
                applyUrl(trimmed, "log.fileIsSub")
                return
            }
            addNodes(parseBody(text), name)
        } catch (e) {
            log("SYS", "warn", t("log.fileReadFail", { error: String(e) }))
        }
    }

    function refreshSub() {
        if (subUpdating) {
            log("SYS", "warn", t("log.subBusy"))
            return
        }
        loadCatalogBlob()
        var g = activeGroup()
        if (!g || !g.url) {
            log("SYS", "warn", t("log.noSubUrl"))
            if (win) win.currentView = "sub"
            if (win && win.settings && typeof win.settings.loadCatalog === "function")
                win.settings.loadCatalog()
            return
        }
        kickFetch(g)
    }

    function updateAllGroups() {
        if (subUpdating) {
            log("SYS", "warn", t("log.subBusy"))
            return
        }
        var data = loadCatalogBlob()
        if (!data || !data.groups) return
        var first = null
        var rest = []
        for (var i = 0; i < data.groups.length; i++) {
            if (!data.groups[i].url) continue
            if (!first) first = data.groups[i]
            else rest.push(data.groups[i])
        }
        if (!first) {
            log("SYS", "warn", t("log.noSubUrl"))
            return
        }
        subQueue = rest
        kickFetch(first)
    }
}
