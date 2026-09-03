import QtQuick

QtObject {
    id: flow

    required property var host
    readonly property var home: host.home

    property bool testing: false
    property bool testAbort: false
    property int testExpect: 0
    property int testGot: 0
    property int testOk: 0
    property int testFail: 0
    property string testLabel: ""
    property var coreTestId: null

    function t(k, v) { return host.t(k, v) }
    function invoke(cmd, payload) { return host.invoke(cmd, payload) }
    function log(tag, cls, msg) { host.log(tag, cls, msg) }

    function parseAddr(addr) {
        if (!addr || addr === "—") return null
        var hostName = "", port = 443
        if (String(addr).charAt(0) === "[") {
            var m = String(addr).match(/^\[([^\]]+)\]:(\d+)$/)
            if (!m) return null
            hostName = m[1]
            port = parseInt(m[2], 10) || 443
        } else {
            var i = String(addr).lastIndexOf(":")
            if (i <= 0) {
                hostName = addr
            } else {
                hostName = addr.slice(0, i)
                port = parseInt(addr.slice(i + 1), 10) || 443
            }
        }
        if (!hostName) return null
        return { host: hostName, port: port }
    }

    function nodeTargets(scope) {
        var table = home && home.table
        if (!table) return []
        var list = []
        if (scope === "group") {
            var raw = table.raw || []
            for (var i = 0; i < raw.length; i++)
                if (raw[i]) list.push(raw[i])
        } else if (table.selectedNodeList) {
            var sel = table.selectedNodeList()
            for (var k = 0; k < sel.length; k++)
                if (sel[k]) list.push(sel[k])
        } else {
            var n = table.nodeByName ? table.nodeByName(table.selectedName) : null
            if (n) list.push(n)
        }
        var out = []
        for (var j = 0; j < list.length; j++) {
            var p = parseAddr(list[j].addr)
            if (!p) continue
            out.push({ id: list[j].name, host: p.host, port: p.port })
        }
        return out
    }

    function persistLats() {
        var table = home && home.table
        if (!table || !table.raw) return
        var data = host.loadCatalogBlob()
        if (!data || !data.profiles) return
        var id = host.gid()
        var prof = data.profiles[id]
        if (!prof || !Array.isArray(prof.nodes)) return
        var byName = {}
        var list = table.raw || []
        for (var i = 0; i < list.length; i++)
            if (list[i] && list[i].name) byName[list[i].name] = list[i]
        for (var j = 0; j < prof.nodes.length; j++) {
            var n = prof.nodes[j]
            if (!n || !n.name) continue
            var src = byName[n.name]
            if (!src || src.lat === "…") continue
            n.lat = (src.lat === undefined) ? n.lat : src.lat
        }
        host.putCatalog(data)
    }

    function finishTest() {
        if (!testing && !testAbort) return
        var extra = testAbort ? t("log.testStopped") : ""
        var via = testLabel === t("log.testViaNode")
        log("OK", "ok", t("log.testDone", {
            label: testLabel || t("tb.test"),
            ok: testOk,
            fail: testFail,
            extra: extra
        }))
        log("SYS", "info", t(via ? "log.testNoteViaNode" : "log.testNote"))
        persistLats()
        testing = false
        testAbort = false
        testExpect = 0
        testGot = 0
        testOk = 0
        testFail = 0
        testLabel = ""
        coreTestId = null
    }

    function coreRunning() {
        var st = invoke("session_status", {})
        var d = st && st.ok ? st.data : null
        return !!(d && d.running)
    }

    function testRun(scope) {
        if (testing) {
            log("TEST", "warn", t("log.testBusy"))
            return
        }
        var targets = nodeTargets(scope)
        if (!targets.length) {
            log("TEST", "warn", t("log.noTestTargets"))
            return
        }
        var live = coreRunning()
        var connectedName = home ? home.connectedName : ""
        if (live) {
            var only = targets.length === 1 && connectedName && targets[0].id === connectedName
            if (!only) {
                log("TEST", "warn", t("log.testViaNodeOnly"))
                host.askConfirm(t("log.testViaNodeOnly"), {
                    title: t("log.testViaNode"),
                    okText: t("btn.ok")
                }, function () {})
                return
            }
            testing = true
            testAbort = false
            testOk = 0
            testFail = 0
            testLabel = t("log.testViaNode")
            log("TEST", "info", t("log.testStartViaNode", { name: connectedName }))
            if (home.table && home.table.setLat)
                home.table.setLat(connectedName, "…")
            var started = invoke("core_url_test_current", {
                url: "https://www.gstatic.com/generate_204",
                timeoutMs: 3000
            })
            var startData = started && started.data ? started.data : started
            if (!started || started.ok === false || !startData || startData.test_id == null) {
                if (home.table && home.table.setLat) home.table.setLat(connectedName, -1)
                testFail = 1
                log("TEST", "warn", t("log.probeUnavailable", { error: (started && started.error) || "probe" }))
                finishTest()
                return
            }
            coreTestId = startData.test_id
            return
        }
        testing = true
        testAbort = false
        testExpect = targets.length
        testGot = 0
        testOk = 0
        testFail = 0
        testLabel = scope === "group" ? t("test.urlGroup") : t("test.urlSelected")
        var conc = Math.min(64, Math.max(16, Math.ceil(targets.length / 6)))
        if (home && home.table && home.table.setLat) {
            for (var i = 0; i < targets.length; i++)
                home.table.setLat(targets[i].id, "…")
        }
        var started = invoke("net_tcp_probe", {
            targets: targets,
            timeoutMs: 3000,
            concurrency: conc
        })
        if (!started || started.ok === false || started.error) {
            testFail = targets.length
            if (home && home.table && home.table.setLat) {
                for (var j = 0; j < targets.length; j++)
                    home.table.setLat(targets[j].id, -1)
            }
            log("TEST", "warn", t("log.probeUnavailable", { error: (started && started.error) || "probe" }))
            finishTest()
            return
        }
        log("TEST", "info", t("log.testStart", { label: testLabel, n: targets.length }))
    }

    function onCoreUrlTestResult(raw) {
        if (!testing || coreTestId == null || !raw || raw.test_id !== coreTestId) return
        var connectedName = home ? home.connectedName : ""
        if (raw.ok === false || raw.error) {
            if (home.table && home.table.setLat) home.table.setLat(connectedName, -1)
            testFail = 1
            log("TEST", "warn", t("log.probeUnavailable", { error: raw.error || "probe" }))
            finishTest()
            return
        }
        var rows = raw.results || []
        var r0 = rows[0] || {}
        var err = String(r0.error || "").trim()
        var ms = Number(r0.ms) || 0
        if (testAbort || err || ms <= 0) {
            if (home.table && home.table.setLat) home.table.setLat(connectedName, -1)
            testFail = 1
            if (err && err !== "test aborted" && !testAbort)
                log("TEST", "warn", "[" + connectedName + "] " + err)
        } else {
            if (home.table && home.table.setLat) home.table.setLat(connectedName, ms)
            testOk = 1
        }
        finishTest()
    }

    function testStop() {
        if (!testing) {
            log("TEST", "info", t("log.noTestJob"))
            return
        }
        testAbort = true
        invoke("net_tcp_probe_stop", {})
        invoke("core_url_test_stop", {})
        log("TEST", "warn", t("log.stoppingTest"))
        var table = home && home.table
        if (table && table.raw && table.setLat) {
            var list = table.raw
            for (var i = 0; i < list.length; i++) {
                if (list[i] && list[i].lat === "…") {
                    table.setLat(list[i].name, -1)
                    testFail += 1
                    testGot += 1
                }
            }
        }
        finishTest()
    }

    function testClear() {
        if (home && home.table && home.table.clearLats)
            home.table.clearLats()
        persistLats()
        log("SYS", "info", t("log.clearedTests"))
    }
}
