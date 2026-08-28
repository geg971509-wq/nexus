pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Window

Item {
    id: root
    // Holder only. A fill Item on Overlay.overlay would steal every click
    // (empty Item still hit-tests). Masks reparent onto the overlay themselves.
    width: 0
    height: 0

    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property var home: win ? win.home : null
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
    readonly property var mono: th ? th.monoFamilies : ["Menlo"]
    readonly property color label: th ? th.label : "#1d1d1f"
    readonly property color secondary: th ? th.secondary : "#6e6e73"
    readonly property color tertiary: th ? th.tertiary : "#8e8e93"
    readonly property color blue: th ? th.blue : "#007aff"
    readonly property color orange: th ? th.orange : "#ff9f0a"
    readonly property color red: th ? th.red : "#ff3b30"
    readonly property color fill: th ? th.fill : "#1e787880"
    readonly property color menuBg: th ? th.menuBg : "#ffffff"
    readonly property color menuBorder: th ? th.menuBorder : "#1a000000"
    readonly property color scrim: th && th.dark ? "#99000000" : "#66000000"
    readonly property int r: th ? th.radius : 6
    readonly property int rLg: th ? th.radiusLg : 8

    property var catalog: null
    property string askTitle: ""
    property string askMsg: ""
    property string askOkText: ""
    property bool askDanger: false
    property bool askUniform: false
    property var askCb: null

    function t(k, v) { return i18 ? i18.t(k, v) : k }

    function api() {
        // nexus is an intentionally injected C++ context property.
        // qmllint disable unqualified
        return (typeof nexus === "undefined") ? null : nexus
        // qmllint enable unqualified
    }

    function parseReply(raw) {
        if (raw === undefined || raw === "") return { ok: false, offline: true }
        if (raw === null) return { ok: true, data: null }
        var obj = raw
        if (typeof raw === "string") {
            try { obj = JSON.parse(raw) } catch (e) { return { ok: false, error: raw } }
        }
        if (obj === null) return { ok: true, data: null }
        if (obj && typeof obj === "object") {
            if (obj.offline) return obj
            if (obj.ok === false) return obj
            if (obj.ok === true) return obj
            var keys = Object.keys(obj)
            if (keys.length === 1 && keys[0] === "error")
                return { ok: false, error: String(obj.error) }
            return { ok: true, data: obj }
        }
        return { ok: true, data: obj }
    }

    function invoke(cmd, payload) {
        var a = api()
        if (!a || typeof a.invoke !== "function") return { ok: false, offline: true }
        try {
            var json = payload == null ? "{}" : (typeof payload === "string" ? payload : JSON.stringify(payload))
            return parseReply(a.invoke(cmd, json))
        } catch (e) {
            return { ok: false, error: String(e) }
        }
    }

    function log(tag, cls, msg) {
        if (home && typeof home.log === "function") home.log(tag, cls, msg)
    }

    function unwrapCatalog(blob) {
        if (!blob || typeof blob !== "object") return null
        if (blob.v === 1 && blob.groups) return blob
        if (blob.data && blob.data.v === 1) return blob.data
        if (blob.catalog && blob.catalog.v === 1) return blob.catalog
        return null
    }

    function loadCatalogBlob() {
        var r = invoke("catalog_get", {})
        catalog = unwrapCatalog(r && r.ok ? (r.data || r) : (r && r.data))
        return catalog
    }

    function putCatalog(blob) {
        return invoke("catalog_put", { blob: blob })
    }

    function reloadHome() {
        if (home && typeof home.loadCatalog === "function")
            home.loadCatalog()
    }

    function gid() {
        if (home && home.activeGid) return home.activeGid
        if (win && win.subTab) return win.subTab
        if (catalog && catalog.active) return catalog.active
        return "default"
    }

    function activeGroup() {
        var data = catalog || loadCatalogBlob()
        if (!data || !data.groups || !data.groups.length)
            return { id: "default", name: "Default", url: "" }
        var id = gid()
        for (var i = 0; i < data.groups.length; i++)
            if (data.groups[i].id === id) return data.groups[i]
        return data.groups[0]
    }

    DialogImportController { id: importFlow; host: root }
    property alias subUpdating: importFlow.subUpdating

    function parsedOf(r) { return importFlow.parsedOf(r) }
    function parseBody(body) { return importFlow.parseBody(body) }
    function logSkipped(parsed) { importFlow.logSkipped(parsed) }
    function addNodes(parsed, src) { importFlow.addNodes(parsed, src) }
    function applyUrl(url, srcKey) { importFlow.applyUrl(url, srcKey) }
    function isSubscriptionUrl(text) { return importFlow.isSubscriptionUrl(text) }
    function kickFetch(g) { return importFlow.kickFetch(g) }
    function onSubFetchResult(raw) { importFlow.onSubFetchResult(raw) }
    function importClip() { importFlow.importClip() }
    function importQrFile(url) { importFlow.importQrFile(url) }
    function importFileUrl(url) { importFlow.importFileUrl(url) }
    function refreshSub() { importFlow.refreshSub() }
    function updateAllGroups() { importFlow.updateAllGroups() }

    function requestQuit() {
        var live = !!(home && home.connected)
        if (!live) {
            var st = invoke("session_status", {})
            var d = st && st.ok ? st.data : null
            live = !!(d && d.running)
        }
        if (live) {
            simpleOverlays.showQuit()
            return
        }
        confirmQuit()
    }

    function confirmQuit() {
        log("SYS", "info", t("log.quitting"))
        var r = invoke("app_quit", { force: true })
        var d = r && r.data ? r.data : r
        simpleOverlays.hideQuit()
        if (d && d.quit === true)
            Qt.quit()
        else
            log("SYS", "warn", t("log.quitFail", { error: (r && r.error) || "quit" }))
    }

    function importFile() { fileDlg.open() }
    function importQr() { qrDlg.open() }

    function hideWin() {
        if (win) win.hide()
        log("SYS", "info", t("log.hiddenMenu"))
    }

    DialogTestController { id: testFlow; host: root }
    property alias testing: testFlow.testing
    property alias testAbort: testFlow.testAbort
    property alias testExpect: testFlow.testExpect
    property alias testGot: testFlow.testGot
    property alias testOk: testFlow.testOk
    property alias testFail: testFlow.testFail
    property alias testLabel: testFlow.testLabel

    function parseAddr(addr) { return testFlow.parseAddr(addr) }
    function nodeTargets(scope) { return testFlow.nodeTargets(scope) }
    function persistLats() { testFlow.persistLats() }
    function finishTest() { testFlow.finishTest() }
    function coreRunning() { return testFlow.coreRunning() }
    function testRun(scope) { testFlow.testRun(scope) }
    function testStop() { testFlow.testStop() }
    function testClear() { testFlow.testClear() }

    DialogNodeController { id: nodeFlow; host: root }
    property alias eOrigName: nodeFlow.eOrigName
    property alias eName: nodeFlow.eName
    property alias eType: nodeFlow.eType
    property alias eServer: nodeFlow.eServer
    property alias ePort: nodeFlow.ePort
    property alias eUuid: nodeFlow.eUuid
    property alias eFlow: nodeFlow.eFlow
    property alias eSecurity: nodeFlow.eSecurity
    property alias eAlterId: nodeFlow.eAlterId
    property alias eUser: nodeFlow.eUser
    property alias ePass: nodeFlow.ePass
    property alias eMethod: nodeFlow.eMethod
    property alias eSni: nodeFlow.eSni
    property alias eNetwork: nodeFlow.eNetwork
    property alias eHost: nodeFlow.eHost
    property alias ePath: nodeFlow.ePath
    property alias eCongest: nodeFlow.eCongest
    property alias eAlpn: nodeFlow.eAlpn
    property alias eNote: nodeFlow.eNote
    property alias eTls: nodeFlow.eTls
    property alias eInsecure: nodeFlow.eInsecure
    property alias qrName: nodeFlow.qrName
    property alias qrLink: nodeFlow.qrLink
    property alias qrSvg: nodeFlow.qrSvg
    property alias resolveGot: nodeFlow.resolveGot
    property alias resolveExpect: nodeFlow.resolveExpect

    function clipSet(text) { return nodeFlow.clipSet(text) }
    function looksLikeUuid(id) { return nodeFlow.looksLikeUuid(id) }
    function isShareUri(s) { return nodeFlow.isShareUri(s) }
    function nodeShareLink(n) { return nodeFlow.nodeShareLink(n) }
    function tableSel() { return nodeFlow.tableSel() }
    function liveProfile(data) { return nodeFlow.liveProfile(data) }
    function putLive(pack) { nodeFlow.putLive(pack) }
    function normalizeType(ty) { return nodeFlow.normalizeType(ty) }
    function editTypeKey(typeLabel) { return nodeFlow.editTypeKey(typeLabel) }
    function eShow(keys) { return nodeFlow.eShow(keys) }
    function fieldsFromOutbound(ob, fallback) { return nodeFlow.fieldsFromOutbound(ob, fallback) }
    function buildOutboundFromFields(f) { return nodeFlow.buildOutboundFromFields(f) }
    function hydrateNode(n) { return nodeFlow.hydrateNode(n) }
    function openEdit(name) { nodeFlow.openEdit(name) }
    function saveEdit() { nodeFlow.saveEdit() }
    function openQr(name) { nodeFlow.openQr(name) }
    function copyQr() { nodeFlow.copyQr() }
    function deleteSelected() { nodeFlow.deleteSelected() }
    function dropNames(sel) { nodeFlow.dropNames(sel) }
    function cloneSelected() { nodeFlow.cloneSelected() }
    function copyLinkSelected() { nodeFlow.copyLinkSelected() }
    function resetTrafficSelected() { nodeFlow.resetTrafficSelected() }
    function dedupeNodes() { nodeFlow.dedupeNodes() }
    function latFailed(n) { return nodeFlow.latFailed(n) }
    function latEmpty(n) { return nodeFlow.latEmpty(n) }
    function removeByPred(pred, label) { nodeFlow.removeByPred(pred, label) }
    function resolveSelected() { nodeFlow.resolveSelected() }
    function applyResolved(id, ip) { nodeFlow.applyResolved(id, ip) }

    function selectEditType(index) { nodeOverlays.selectEditType(index) }
    function showEditDialog() { nodeOverlays.showEdit() }
    function hideEditDialog() { nodeOverlays.hideEdit() }
    function showQrDialog() { simpleOverlays.showQr() }

    function askConfirm(msg, opts, cb) {
        opts = opts || {}
        askTitle = opts.title || t("confirm.askTitle")
        askMsg = msg || ""
        askOkText = opts.okText || t("btn.ok")
        askDanger = !!opts.danger
        askUniform = !!opts.uniform
        askCb = cb
        simpleOverlays.showConfirm()
    }

    function closeAsk(ok) {
        simpleOverlays.hideConfirm()
        var cb = askCb
        askCb = null
        if (typeof cb === "function") cb(!!ok)
    }

    function openNodeCtx(gx, gy) { nodeOverlays.openContext(gx, gy) }

    function ctxAct(act) {
        nodeOverlays.closeContext()
        var sel = tableSel()
        var one = sel.length ? sel[0].name : (home && home.selectedName)
        if (act === "add-clip") importClip()
        else if (act === "add-file") importFile()
        else if (act === "scan-qr") importQr()
        else if (act === "edit") openEdit(one)
        else if (act === "start") {
            if (home && typeof home.startNamed === "function") home.startNamed(one)
        } else if (act === "stop") {
            if (home && typeof home.stopTunnel === "function") home.stopTunnel()
        } else if (act === "clone") cloneSelected()
        else if (act === "delete") deleteSelected()
        else if (act === "copy-link") copyLinkSelected()
        else if (act === "show-qr") openQr(one)
        else if (act === "select-all") {
            if (home && home.table && home.table.selectAll) home.table.selectAll()
        } else if (act === "refresh-sub") refreshSub()
        else if (act === "url-test") testRun("selected")
        else if (act === "resolve-ip") resolveSelected()
        else if (act === "clear-test") testClear()
        else if (act === "reset-traffic") resetTrafficSelected()
        else if (act === "dedupe") dedupeNodes()
        else if (act === "rm-unavailable") removeByPred(latFailed, t("ctx.rmUnavailable"))
        else if (act === "rm-failed") removeByPred(latEmpty, t("ctx.rmFailed"))
    }

    DialogGroupController { id: groupFlow; host: root }
    property alias creatingGroup: groupFlow.creatingGroup
    property alias editId: groupFlow.editId
    property alias groupEditError: groupFlow.groupEditError

    function openGroups() { groupFlow.openGroups() }
    function openGroupEdit(id, mode) { groupFlow.openGroupEdit(id, mode) }
    function saveGroupEdit() { groupFlow.saveGroupEdit() }
    function groupLiveNodeName(data, id) { return groupFlow.groupLiveNodeName(data, id) }
    function deleteGroup(id) { groupFlow.deleteGroup(id) }
    function deleteGroupConfirmed(id) { groupFlow.deleteGroupConfirmed(id) }

    function setGroupRows(rows) {
        groupOverlays.setRows(rows)
    }
    function showGroupsDialog() { groupOverlays.showGroups() }
    function setGroupEditForm(title, subtitle, name, url) {
        groupOverlays.setEditForm(title, subtitle, name, url)
    }
    function showGroupEditDialog() {
        groupOverlays.showEdit()
    }
    function hideGroupEditDialog() { groupOverlays.hideEdit() }
    function focusGroupEditName() { groupOverlays.focusName() }
    function focusGroupEditUrl() { groupOverlays.focusUrl() }
    function groupEditName() { return groupOverlays.editName() }
    function groupEditUrl() { return groupOverlays.editUrl() }

    DialogStatusController { id: statusFlow; host: root }
    property alias stats: statusFlow.stats
    property alias exportText: statusFlow.exportText

    function countryLabel(cc) { return statusFlow.countryLabel(cc) }
    function fillStats() { statusFlow.fillStats() }
    function openStats() { statusFlow.openStats() }
    function openExport() { statusFlow.openExport() }
    function copyExport() { statusFlow.copyExport() }
    function showStatsDialog() { simpleOverlays.showStats() }
    function showExportDialog() { simpleOverlays.showExport() }

    Connections {
        target: root.api()
        function onQuitRequested() {
            root.requestQuit()
        }
        function onEvent(name, json) {
            var r = json
            if (typeof json === "string") {
                try { r = JSON.parse(json) } catch (e) { return }
            }
            if (r && r.payload !== undefined) r = r.payload
            if (name === "net-probe-result") {
                if (!r || r.id == null) return
                var fail = !r.ok || r.ms == null || r.ms < 0
                if (root.home && root.home.table && root.home.table.setLat) {
                    if (fail)
                        root.home.table.setLat(r.id, -1)
                    else
                        root.home.table.setLat(r.id, r.ms)
                }
                if (root.testing) {
                    if (fail) {
                        root.testFail += 1
                        if (r.error && r.error !== "aborted" && r.error !== "test aborted" && root.testFail <= 5)
                            root.log("TEST", "warn", "[" + r.id + "] " + r.error)
                    } else {
                        root.testOk += 1
                    }
                    root.testGot += 1
                    if (root.testGot === root.testExpect || (root.testGot > 0 && root.testGot % 25 === 0))
                        root.log("TEST", "info", root.testLabel + " " + root.testGot + "/" + root.testExpect + " · ok " + root.testOk + " · fail " + root.testFail)
                    if (root.testGot >= root.testExpect)
                        root.finishTest()
                }
                return
            }
            if (name === "net-resolve-result") {
                if (!r || r.id == null) return
                if (r.ok && r.ips && r.ips.length)
                    root.applyResolved(r.id, r.ips[0])
                else
                    root.log("SYS", "warn", root.t("log.exitIpFail", { id: r.id, error: r.error || root.t("log.noAddr") }))
                if (root.resolveExpect > 0) {
                    root.resolveGot += 1
                    if (root.resolveGot >= root.resolveExpect) {
                        root.resolveExpect = 0
                        root.resolveGot = 0
                    }
                }
                return
            }
            if (name === "sub-fetch-result")
                root.onSubFetchResult(r)
        }
    }

    FileDialog {
        id: fileDlg
        fileMode: FileDialog.OpenFile
        nameFilters: ["Config (*.txt *.json *.yaml *.yml *.conf *.sub)", "All files (*)"]
        onAccepted: root.importFileUrl(selectedFile)
    }
    FileDialog {
        id: qrDlg
        fileMode: FileDialog.OpenFile
        nameFilters: ["Images (*.png *.jpg *.jpeg *.heic *.tif *.tiff *.gif *.bmp)", "All files (*)"]
        onAccepted: root.importQrFile(selectedFile)
    }

    DialogSimpleOverlays { id: simpleOverlays; host: root }
    DialogGroupOverlays { id: groupOverlays; host: root }
    DialogNodeOverlays { id: nodeOverlays; host: root }











}
