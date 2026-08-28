import QtQuick

QtObject {
    id: flow

    required property var host
    readonly property var win: host.win
    readonly property var home: host.home

    property var stats: ({
        core: "—", conn: "—", proxy: "—", direct: "—",
        uptime: "—", ip: "—", country: "—", next: "—"
    })
    property string exportText: ""

    function t(k, v) { return host.t(k, v) }
    function api() { return host.api() }
    function invoke(cmd, payload) { return host.invoke(cmd, payload) }
    function activeGroup() { return host.activeGroup() }
    function log(tag, cls, msg) { host.log(tag, cls, msg) }

    function countryLabel(cc) {
        var c = String(cc || "").trim().toUpperCase()
        if (!/^[A-Z]{2}$/.test(c)) return ""
        return c
    }

    function fillStats() {
        var core = t("stats.coreStopped")
        var live = false
        var st = invoke("session_status", {})
        var d = st && st.ok ? st.data : null
        if (d && d.running) {
            live = true
            core = (d.profile_id != null && d.profile_id >= 0)
                   ? t("stats.coreRunningPid", { id: d.profile_id })
                   : t("stats.coreRunning")
        } else if (d && d.process_alive) {
            core = t("stats.coreAliveIdle")
        }
        var proxy = "—", direct = "—"
        if (live && win) {
            if (win.sbProxy && win.sbProxy !== "—") proxy = win.sbProxy
            if (win.sbDirect && win.sbDirect !== "—") direct = win.sbDirect
        }
        var ip = "—", country = "—"
        if (live) {
            var ex = invoke("exit_ip_probe", {})
            var ed = ex && ex.ok ? (ex.data || ex) : null
            if (ed && ed.ip) ip = ed.ip
            if (ed && ed.country) country = countryLabel(ed.country) || "—"
        }
        var g = activeGroup()
        var next = (g && g.url) ? t("stats.nextSubPending") : "—"
        var uptime = "—"
        if (live && home && home.connectedAt) {
            var sec = Math.floor((Date.now() - Number(home.connectedAt)) / 1000)
            if (sec < 0) sec = 0
            var h = Math.floor(sec / 3600)
            var m = Math.floor((sec % 3600) / 60)
            var s = sec % 60
            function pad(n) { return (n < 10 ? "0" : "") + n }
            uptime = h + "h " + pad(m) + "m " + pad(s) + "s"
        }
        stats = {
            core: core,
            conn: "—",
            proxy: live ? proxy : "—",
            direct: live ? direct : "—",
            uptime: uptime,
            ip: ip,
            country: country,
            next: next
        }
    }

    function openStats() {
        fillStats()
        host.showStatsDialog()
    }

    function openExport() {
        exportText = t("js.generating")
        host.showExportDialog()
        var payload = null
        if (home && typeof home.connectPayload === "function")
            payload = home.connectPayload(home.selectedName)
        if (!payload) {
            exportText = t("js.noNodePayload")
            log("SYS", "warn", t("js.noNodePayload"))
            return
        }
        var r = invoke("generate_preview", payload)
        if (!r || r.ok === false) {
            exportText = String((r && r.error) || t("log.previewFail", { error: "preview" }))
            log("SYS", "warn", t("log.previewFail", { error: (r && r.error) || "preview" }))
            return
        }
        try {
            exportText = JSON.stringify(r.data !== undefined ? r.data : r, null, 2)
            log("SYS", "ok", t("log.previewExported"))
        } catch (e) {
            exportText = String(r.data || r)
        }
    }

    function copyExport() {
        var a = api()
        if (!a || typeof a.setClipboardText !== "function") {
            log("SYS", "warn", t("log.copyFail"))
            return
        }
        a.setClipboardText(exportText)
        log("SYS", "ok", t("log.sbCopied"))
    }

}
