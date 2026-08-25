import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
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

    property bool testing: false
    property bool testAbort: false
    property int testExpect: 0
    property int testGot: 0
    property int testOk: 0
    property int testFail: 0
    property string testLabel: ""
    property bool subUpdating: false
    property var subJob: null
    property var subQueue: []
    property var catalog: null
    property bool creatingGroup: false
    property string editId: ""
    property string groupEditError: ""
    property var stats: ({
        core: "—", conn: "—", proxy: "—", direct: "—",
        uptime: "—", ip: "—", country: "—", next: "—"
    })
    property string exportText: ""
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
    property string askTitle: ""
    property string askMsg: ""
    property string askOkText: ""
    property bool askDanger: false
    property bool askUniform: false
    property var askCb: null
    property int resolveGot: 0
    property int resolveExpect: 0

    readonly property var extra: ({
        "ctx.addClip": { "zh-CN": "添加剪贴板中的配置档", "en": "Add Profile from Clipboard", "ru": "Добавить профиль из буфера", "zh-TW": "從剪貼簿新增設定檔" },
        "ctx.addFile": { "zh-CN": "添加文件中的配置档", "en": "Add Profile from File", "ru": "Добавить профиль из файла", "zh-TW": "從檔案新增設定檔" },
        "ctx.clone": { "zh-CN": "克隆", "en": "Clone", "ru": "Клонировать", "zh-TW": "複製節點" },
        "ctx.copyLink": { "zh-CN": "复制链接", "en": "Copy Link", "ru": "Копировать ссылку", "zh-TW": "複製連結" },
        "ctx.dedupe": { "zh-CN": "去除重复节点", "en": "Remove Duplicates", "ru": "Удалить дубликаты", "zh-TW": "移除重複節點" },
        "ctx.delete": { "zh-CN": "删除", "en": "Delete", "ru": "Удалить", "zh-TW": "刪除" },
        "ctx.edit": { "zh-CN": "编辑", "en": "Edit", "ru": "Изменить", "zh-TW": "編輯" },
        "ctx.refreshSub": { "zh-CN": "更新订阅", "en": "Update Subscription", "ru": "Обновить подписку", "zh-TW": "更新訂閱" },
        "ctx.resetTraffic": { "zh-CN": "重置流量", "en": "Reset Traffic", "ru": "Сбросить трафик", "zh-TW": "重置流量" },
        "ctx.resolveIp": { "zh-CN": "解析选定的出口 IP", "en": "Resolve Exit IP", "ru": "Определить выходной IP", "zh-TW": "解析選定的出口 IP" },
        "ctx.rmFailed": { "zh-CN": "移除失败/无延迟", "en": "Remove Failed / No Latency", "ru": "Удалить сбой / без задержки", "zh-TW": "移除失敗/無延遲" },
        "ctx.rmUnavailable": { "zh-CN": "移除不可用", "en": "Remove Unavailable", "ru": "Удалить недоступные", "zh-TW": "移除不可用" },
        "ctx.selectAll": { "zh-CN": "全选", "en": "Select All", "ru": "Выбрать все", "zh-TW": "全選" },
        "ctx.showQr": { "zh-CN": "显示二维码", "en": "Show QR", "ru": "Показать QR", "zh-TW": "顯示 QR" },
        "ctx.start": { "zh-CN": "启动", "en": "Start", "ru": "Запустить", "zh-TW": "啟動" },
        "ctx.stop": { "zh-CN": "停止", "en": "Stop", "ru": "Остановить", "zh-TW": "停止" },
        "edit.title": { "zh-CN": "编辑节点", "en": "Edit Node", "ru": "Изменить узел", "zh-TW": "編輯節點" },
        "edit.type": { "zh-CN": "类型", "en": "Type", "ru": "Тип", "zh-TW": "類型" },
        "edit.server": { "zh-CN": "服务器地址", "en": "Server", "ru": "Сервер", "zh-TW": "伺服器位址" },
        "edit.port": { "zh-CN": "端口", "en": "Port", "ru": "Порт", "zh-TW": "連接埠" },
        "edit.user": { "zh-CN": "用户名", "en": "Username", "ru": "Имя пользователя", "zh-TW": "使用者名稱" },
        "edit.pass": { "zh-CN": "密码", "en": "Password", "ru": "Пароль", "zh-TW": "密碼" },
        "edit.method": { "zh-CN": "加密方法", "en": "Method", "ru": "Метод", "zh-TW": "加密方法" },
        "edit.security": { "zh-CN": "加密", "en": "Security", "ru": "Шифрование", "zh-TW": "加密" },
        "edit.congest": { "zh-CN": "拥塞控制", "en": "Congestion", "ru": "Контроль перегрузки", "zh-TW": "擁塞控制" },
        "edit.sni": { "zh-CN": "SNI / server_name", "en": "SNI / server_name", "ru": "SNI / server_name", "zh-TW": "SNI / server_name" },
        "edit.network": { "zh-CN": "传输 network", "en": "Network", "ru": "Транспорт", "zh-TW": "傳輸 network" },
        "edit.host": { "zh-CN": "Host 头", "en": "Host header", "ru": "Host", "zh-TW": "Host 頭" },
        "edit.path": { "zh-CN": "路径 path", "en": "Path", "ru": "Путь", "zh-TW": "路徑 path" },
        "edit.tls": { "zh-CN": "启用 TLS / WSS", "en": "Enable TLS / WSS", "ru": "Включить TLS / WSS", "zh-TW": "啟用 TLS / WSS" },
        "edit.insecure": { "zh-CN": "跳过证书校验 (insecure)", "en": "Skip cert verify (insecure)", "ru": "Не проверять сертификат", "zh-TW": "略過憑證檢查 (insecure)" },
        "edit.note": { "zh-CN": "备注", "en": "Note", "ru": "Заметка", "zh-TW": "備註" },
        "qr.empty": { "zh-CN": "无可编码内容", "en": "Nothing to encode", "ru": "Нечего кодировать", "zh-TW": "無可編碼內容" },
        "qr.noShare": { "zh-CN": "无真实分享链接", "en": "No real share link", "ru": "Нет ссылки", "zh-TW": "無真實分享連結" },
        "log.noNode": { "zh-CN": "未选定节点", "en": "No node selected", "ru": "Узел не выбран", "zh-TW": "未選定節點" },
        "log.deletedN": { "zh-CN": "已删除 {n} 个节点", "en": "Deleted {n} nodes", "ru": "Удалено {n} узлов", "zh-TW": "已刪除 {n} 個節點" },
        "log.resetTraffic": { "zh-CN": "已重置流量 · {n} 个节点", "en": "Reset traffic · {n} nodes", "ru": "Сброшен трафик · {n}", "zh-TW": "已重置流量 · {n} 個節點" },
        "log.cloned": { "zh-CN": "已克隆：{name}", "en": "Cloned: {name}", "ru": "Клонировано: {name}", "zh-TW": "已複製：{name}" },
        "log.copiedLinkNamed": { "zh-CN": "已复制链接 · {name}", "en": "Copied link · {name}", "ru": "Ссылка скопирована · {name}", "zh-TW": "已複製連結 · {name}" },
        "log.noDupes": { "zh-CN": "无重复节点", "en": "No duplicates", "ru": "Нет дубликатов", "zh-TW": "無重複節點" },
        "log.deduped": { "zh-CN": "已去除重复节点 · {n} 个", "en": "Removed {n} duplicates", "ru": "Удалено дубликатов: {n}", "zh-TW": "已移除重複節點 · {n} 個" },
        "log.removed0": { "zh-CN": "{label} · 0 个", "en": "{label} · 0", "ru": "{label} · 0", "zh-TW": "{label} · 0 個" },
        "log.removedN": { "zh-CN": "{label} · {n} 个", "en": "{label} · {n}", "ru": "{label} · {n}", "zh-TW": "{label} · {n} 個" },
        "log.remove": { "zh-CN": "移除", "en": "Remove", "ru": "Удалить", "zh-TW": "移除" },
        "log.noRealLink": { "zh-CN": "选定节点无真实分享链接", "en": "Selected node has no real share link", "ru": "Нет реальной ссылки", "zh-TW": "選定節點無真實分享連結" },
        "log.nodeSaved": { "zh-CN": "已保存节点：{name}", "en": "Saved node: {name}", "ru": "Узел сохранён: {name}", "zh-TW": "已儲存節點：{name}" },
        "log.noLinkCopy": { "zh-CN": "没有可复制的真实链接", "en": "No real link to copy", "ru": "Нет ссылки для копирования", "zh-TW": "沒有可複製的真實連結" },
        "log.linkCopied": { "zh-CN": "链接已复制", "en": "Link copied", "ru": "Ссылка скопирована", "zh-TW": "連結已複製" },
        "log.qrFail": { "zh-CN": "二维码失败：{error}", "en": "QR failed: {error}", "ru": "QR ошибка: {error}", "zh-TW": "QR 失敗：{error}" },
        "log.exitIpBusy": { "zh-CN": "出口 IP 解析进行中…", "en": "Exit IP resolve in progress…", "ru": "Определение IP…", "zh-TW": "出口 IP 解析進行中…" },
        "log.noValidAddr": { "zh-CN": "选定行无有效地址", "en": "No valid address on selection", "ru": "Нет адреса", "zh-TW": "選定列無有效位址" },
        "log.exitIpStart": { "zh-CN": "解析出口 IP · {n} 个…", "en": "Resolve exit IP · {n}…", "ru": "IP · {n}…", "zh-TW": "解析出口 IP · {n} 個…" },
        "log.exitIpFail": { "zh-CN": "解析失败 · {id}：{error}", "en": "Resolve failed · {id}: {error}", "ru": "Ошибка · {id}: {error}", "zh-TW": "解析失敗 · {id}：{error}" },
        "log.noAddr": { "zh-CN": "无地址", "en": "No address", "ru": "Нет адреса", "zh-TW": "無位址" },
        "confirm.deleteNodes": { "zh-CN": "删除选定的 {n} 个节点？", "en": "Delete {n} selected nodes?", "ru": "Удалить выбранные узлы ({n})?", "zh-TW": "刪除選定的 {n} 個節點？" },
        "confirm.deleteNodesTitle": { "zh-CN": "删除节点", "en": "Delete nodes", "ru": "Удалить узлы", "zh-TW": "刪除節點" },
        "conn.copyHost": { "zh-CN": "复制域名/IP", "en": "Copy Domain/IP", "ru": "Копировать домен/IP", "zh-TW": "複製網域/IP" },
        "conn.copyRow": { "zh-CN": "复制整行", "en": "Copy Row", "ru": "Копировать строку", "zh-TW": "複製整列" },
        "js.nodeCopy": { "zh-CN": "{name} 副本", "en": "{name} copy", "ru": "{name} копия", "zh-TW": "{name} 副本" },
        "js.unnamed": { "zh-CN": "未命名", "en": "Unnamed", "ru": "Без имени", "zh-TW": "未命名" },
        "js.generating": { "zh-CN": "生成中…", "en": "Generating…", "ru": "Создание…", "zh-TW": "產生中…" },
        "log.clear": { "zh-CN": "清除", "en": "Clear", "ru": "Очистить", "zh-TW": "清除" },
        "log.copy": { "zh-CN": "复制", "en": "Copy", "ru": "Копировать", "zh-TW": "複製" }
    })

    function lang() { return i18 && i18.lang ? i18.lang : "zh-CN" }

    function t(k, v) {
        var s = i18 ? i18.t(k, v) : k
        if (!s || s === k) {
            var e = extra[k]
            s = (e && (e[lang()] || e["zh-CN"])) || k
            if (v) {
                for (var p in v)
                    s = s.replace("{" + p + "}", v[p])
            }
        }
        return s
    }

    function api() {
        return (typeof nexus === "undefined") ? null : nexus
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

    function nodesOf(r) {
        if (!r || r.ok === false || r.offline) return []
        var d = r.data !== undefined ? r.data : r
        var list = (d && d.nodes) ? d.nodes : (r.nodes || [])
        return Array.isArray(list) ? list : []
    }

    function parseBody(body) {
        var text = String(body || "").trim()
        if (!text) return []
        if (/^<!DOCTYPE\s+html/i.test(text) || /^<html[\s>]/i.test(text)) return []
        var nodes = nodesOf(invoke("sub_parse_share", { body: text }))
        if (nodes.length) return nodes
        try {
            var dec = Qt.atob(String(text).replace(/\s/g, ""))
            if (dec && dec !== text) {
                nodes = nodesOf(invoke("sub_parse_share", { body: dec }))
                if (nodes.length) return nodes
                text = String(dec).trim()
            }
        } catch (e) { /* not b64 */ }
        if (/proxies\s*:/i.test(text)) {
            nodes = nodesOf(invoke("sub_parse_clash", { body: text }))
            if (nodes.length) return nodes
        }
        return []
    }

    function addNodes(nodes, src) {
        if (!nodes || !nodes.length) {
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
            subUpdating = !!(subJob)
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
        var next = parseBody(body)
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
            var o = before[j]
            if (o && o.name) byName[o.name] = o
            if (o && o.addr) byAddr[o.addr] = o
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

    function requestQuit() {
        var live = !!(home && home.connected)
        if (!live) {
            var st = invoke("session_status", {})
            var d = st && st.ok ? st.data : null
            live = !!(d && d.running)
        }
        if (live) {
            quitMask.visible = true
            return
        }
        confirmQuit()
    }

    function confirmQuit() {
        log("SYS", "info", t("log.quitting"))
        var r = invoke("app_quit", { force: true })
        var d = r && r.data ? r.data : r
        quitMask.visible = false
        if (d && d.quit === true)
            Qt.quit()
        else
            log("SYS", "warn", t("log.quitFail", { error: (r && r.error) || "quit" }))
    }

    function importClip() {
        var a = api()
        if (!a || typeof a.clipboardText !== "function") {
            log("SYS", "warn", t("log.clipFail"))
            return
        }
        var text = String(a.clipboardText() || "").trim()
        if (!text) { log("SYS", "warn", t("log.clipEmpty")); return }
        if (/^https?:\/\//i.test(text) && text.indexOf("\n") < 0 && text.length < 500) {
            applyUrl(text, "log.clipIsSub")
            return
        }
        addNodes(parseBody(text), t("js.clipboard"))
    }

    function importFile() { fileDlg.open() }
    function importQr() { qrDlg.open() }

    function importFileUrl(url) {
        try {
            var text = readLocal(url)
            var name = String(url).split("/").pop()
            var trimmed = String(text || "").trim()
            if (/^https?:\/\//i.test(trimmed) && trimmed.indexOf("\n") < 0 && trimmed.length < 500) {
                applyUrl(trimmed, "log.fileIsSub")
                return
            }
            addNodes(parseBody(text), name)
        } catch (e) {
            log("SYS", "warn", t("log.fileReadFail", { error: String(e) }))
        }
    }

    function hideWin() {
        if (win) win.hide()
        log("SYS", "info", t("log.hiddenMenu"))
    }

    function restartCore() {
        log("SYS", "info", t("log.restartingCore"))
    }

    function refreshSub() {
        if (subUpdating) {
            log("SYS", "warn", t("log.subBusy"))
            return
        }
        var data = loadCatalogBlob()
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

    function parseAddr(addr) {
        if (!addr || addr === "—") return null
        var host = "", port = 443
        if (String(addr).charAt(0) === "[") {
            var m = String(addr).match(/^\[([^\]]+)\]:(\d+)$/)
            if (!m) return null
            host = m[1]; port = parseInt(m[2], 10) || 443
        } else {
            var i = String(addr).lastIndexOf(":")
            if (i <= 0) { host = addr; port = 443 }
            else { host = addr.slice(0, i); port = parseInt(addr.slice(i + 1), 10) || 443 }
        }
        if (!host) return null
        return { host: host, port: port }
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
        var data = loadCatalogBlob()
        if (!data || !data.profiles) return
        var id = gid()
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
            if (!src) continue
            if (src.lat === "…") continue
            n.lat = (src.lat === undefined) ? n.lat : src.lat
        }
        putCatalog(data)
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
                askConfirm(t("log.testViaNodeOnly"), {
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
            var res = invoke("core_url_test_current", { url: "https://www.gstatic.com/generate_204", timeoutMs: 3000 })
            if (!res || res.ok === false) {
                if (home.table && home.table.setLat) home.table.setLat(connectedName, -1)
                testFail = 1
                log("TEST", "warn", t("log.probeUnavailable", { error: (res && res.error) || "probe" }))
                finishTest()
                return
            }
            var rows = (res.data && res.data.results) ? res.data.results : []
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
        var started = invoke("net_tcp_probe", { targets: targets, timeoutMs: 3000, concurrency: conc })
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

    readonly property string eKey: editTypeKey(eType)
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
        var nodes = nodesOf(r)
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
        eTypeBox.currentIndex = ti < 0 ? 0 : ti
        editMask.visible = true
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
        editMask.visible = false
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
        qrMask.visible = true
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

    function askConfirm(msg, opts, cb) {
        opts = opts || {}
        askTitle = opts.title || t("confirm.askTitle")
        askMsg = msg || ""
        askOkText = opts.okText || t("btn.ok")
        askDanger = !!opts.danger
        askUniform = !!opts.uniform
        askCb = cb
        askMask.visible = true
    }

    function closeAsk(ok) {
        askMask.visible = false
        var cb = askCb
        askCb = null
        if (typeof cb === "function") cb(!!ok)
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

    function openNodeCtx(gx, gy) {
        var ov = Overlay.overlay
        var p = ov ? ov.mapFromGlobal(gx, gy) : Qt.point(gx, gy)
        nodeCtx.selCount = tableSel().length
        nodeCtx.placeAt(p.x, p.y)
        nodeCtx.open()
    }

    function ctxAct(act) {
        nodeCtx.close()
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

    function openGroups() {
        loadCatalogBlob()
        groupModel.clear()
        var gs = (catalog && catalog.groups) ? catalog.groups : []
        for (var i = 0; i < gs.length; i++) {
            var g = gs[i]
            groupModel.append({ gid: g.id || "", name: g.name || "", count: g.count || 0 })
        }
        groupsMask.visible = true
    }

    function openGroupEdit(id, mode) {
        creatingGroup = mode === "create"
        editId = id || ""
        groupEditError = ""
        var g = null
        if (id && catalog && catalog.groups) {
            for (var i = 0; i < catalog.groups.length; i++)
                if (catalog.groups[i].id === id) g = catalog.groups[i]
        }
        if (creatingGroup) {
            geTitle.text = t("js.newGroup")
            geSub.text = t("js.newGroupSub")
            geName.text = ""
            geUrl.text = ""
        } else {
            if (!g) return
            geTitle.text = t("js.editGroup")
            geSub.text = t("js.editGroupSub")
            geName.text = g.name || ""
            geUrl.text = g.url || ""
        }
        groupEditMask.visible = true
        geName.forceActiveFocus()
        geName.selectAll()
    }

    function saveGroupEdit() {
        var name = geName.text.trim()
        groupEditError = ""
        if (!name) {
            groupEditError = t("log.groupNameEmpty")
            geName.forceActiveFocus()
            return
        }
        var data = loadCatalogBlob()
        if (!data)
            data = { v: 1, active: "default", groups: [], profiles: {} }
        if (!data.groups) data.groups = []
        if (!data.profiles) data.profiles = {}
        for (var i = 0; i < data.groups.length; i++) {
            if (data.groups[i].id !== editId && data.groups[i].name === name) {
                groupEditError = t("log.groupNameDup")
                geName.forceActiveFocus()
                return
            }
        }
        var url = geUrl.text.trim()
        var created = creatingGroup
        if (created) {
            var nid = "g" + Date.now().toString(36)
            data.groups.push({ id: nid, name: name, url: url, count: 0 })
            data.profiles[nid] = { label: name, nodes: [] }
            data.active = nid
        } else {
            var g = null
            for (var j = 0; j < data.groups.length; j++)
                if (data.groups[j].id === editId) { g = data.groups[j]; break }
            if (!g) return
            g.name = name
            g.url = url
            if (data.profiles[g.id]) data.profiles[g.id].label = name
        }
        var saved = putCatalog(data)
        if (!saved || saved.offline || saved.ok === false) {
            groupEditError = String((saved && saved.error) || "save")
            return
        }
        groupEditMask.visible = false
        openGroups()
        reloadHome()
        log("SYS", "ok", t(created ? "log.groupCreated" : "log.groupSaved", {
            name: name,
            url: url ? " · " + url : ""
        }))
    }

    function groupLiveNodeName(data, id) {
        var profile = data && data.profiles ? data.profiles[id] : null
        var nodes = profile && Array.isArray(profile.nodes) ? profile.nodes : []
        if (!nodes.length || !home) return ""
        var live = []
        if (home.connected && home.connectedName) live.push(home.connectedName)
        if (home.powerBusy && home.selectedName && home.selectedName !== "—")
            live.push(home.selectedName)
        for (var i = 0; i < nodes.length; i++) {
            var name = nodes[i] && nodes[i].name
            if (name && live.indexOf(name) >= 0) return name
        }
        return ""
    }

    function deleteGroup(id) {
        var data = loadCatalogBlob()
        if (!data || !data.groups) return
        var g = null
        for (var i = 0; i < data.groups.length; i++)
            if (data.groups[i].id === id) { g = data.groups[i]; break }
        if (!g) return
        if (data.groups.length <= 1) {
            log("SYS", "warn", t("log.keepOneGroup"))
            return
        }
        var live = groupLiveNodeName(data, id)
        if (live) {
            askConfirm(t("confirm.deleteGroupLive", { name: g.name, node: live }), {
                title: t("confirm.deleteGroupTitle"),
                okText: t("btn.ok"),
                uniform: true
            }, function () {})
            log("SYS", "warn", t("log.groupLiveInUse", { name: g.name, node: live }))
            return
        }
        askConfirm(t("confirm.deleteGroup", { name: g.name }), {
            title: t("confirm.deleteGroupTitle"),
            okText: t("ctx.delete"),
            danger: true,
            uniform: true
        }, function (ok) { if (ok) deleteGroupConfirmed(id) })
    }

    function deleteGroupConfirmed(id) {
        var data = loadCatalogBlob()
        if (!data || !data.groups || data.groups.length <= 1) return
        var g = null
        for (var i = 0; i < data.groups.length; i++)
            if (data.groups[i].id === id) { g = data.groups[i]; break }
        if (!g) return
        var live = groupLiveNodeName(data, id)
        if (live) {
            log("SYS", "warn", t("log.groupLiveInUse", { name: g.name, node: live }))
            return
        }
        var next = []
        for (var j = 0; j < data.groups.length; j++)
            if (data.groups[j].id !== id) next.push(data.groups[j])
        data.groups = next
        if (data.profiles) delete data.profiles[id]
        if (data.active === id) data.active = next[0].id
        var saved = putCatalog(data)
        if (!saved || saved.offline || saved.ok === false) return
        openGroups()
        reloadHome()
        log("SYS", "warn", t("log.groupDeleted", { name: g.name }))
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

    function fmtBytes(n) {
        n = Number(n) || 0
        if (n < 1024) return n + " B"
        if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB"
        if (n < 1024 * 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + " MB"
        return (n / (1024 * 1024 * 1024)).toFixed(2) + " GB"
    }

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
        statsMask.visible = true
    }

    function openExport() {
        exportText = t("js.generating")
        exportMask.visible = true
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

    Connections {
        target: (typeof nexus === "undefined") ? null : nexus
        function onEvent(name, json) {
            var r = json
            if (typeof json === "string") {
                try { r = JSON.parse(json) } catch (e) { return }
            }
            if (r && r.payload !== undefined) r = r.payload
            if (name === "net-probe-result") {
                if (!r || r.id == null) return
                var fail = !r.ok || r.ms == null || r.ms < 0
                if (home && home.table && home.table.setLat) {
                    if (fail)
                        home.table.setLat(r.id, -1)
                    else
                        home.table.setLat(r.id, r.ms)
                }
                if (testing) {
                    if (fail) {
                        testFail += 1
                        if (r.error && r.error !== "aborted" && r.error !== "test aborted" && testFail <= 5)
                            log("TEST", "warn", "[" + r.id + "] " + r.error)
                    } else {
                        testOk += 1
                    }
                    testGot += 1
                    if (testGot === testExpect || (testGot > 0 && testGot % 25 === 0))
                        log("TEST", "info", testLabel + " " + testGot + "/" + testExpect + " · ok " + testOk + " · fail " + testFail)
                    if (testGot >= testExpect)
                        finishTest()
                }
                return
            }
            if (name === "net-resolve-result") {
                if (!r || r.id == null) return
                if (r.ok && r.ips && r.ips.length)
                    root.applyResolved(r.id, r.ips[0])
                else
                    root.log("SYS", "warn", root.t("log.exitIpFail", { id: r.id, error: r.error || root.t("log.noAddr") }))
                if (resolveExpect > 0) {
                    resolveGot += 1
                    if (resolveGot >= resolveExpect) {
                        resolveExpect = 0
                        resolveGot = 0
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
        nameFilters: ["Images (*.png *.jpg *.jpeg *.webp *.gif *.bmp)", "All files (*)"]
        onAccepted: {
            var u = String(selectedFile)
            var name = u.split("/").pop()
            root.log("SYS", "info", root.t("log.imagePicked", { name: name }))
        }
    }

    ListModel { id: groupModel }

    component Mask: Rectangle {
        id: mask
        property var dismiss: function () { mask.visible = false }
        parent: Overlay.overlay
        anchors.fill: parent
        z: 400
        color: root.scrim
        visible: false
        enabled: visible
        MouseArea { anchors.fill: parent; enabled: mask.visible; onClicked: mask.dismiss() }
    }
    component Card: Rectangle {
        property int cardW: 360
        width: Math.min(cardW, parent.width - 40)
        radius: root.rLg
        color: root.menuBg
        border.width: 1
        border.color: root.menuBorder
        anchors.centerIn: parent
        MouseArea { anchors.fill: parent }
    }
    component DBtn: AbstractButton {
        id: btn
        property bool primary: false
        property bool danger: false
        property bool uniform: false
        height: 30
        implicitWidth: uniform ? 112 : Math.max(72, txt.implicitWidth + 32)
        hoverEnabled: true
        opacity: enabled ? 1 : 0.45
        focusPolicy: Qt.StrongFocus
        Accessible.name: text
        background: Rectangle {
            radius: uniform ? root.r : 8
            color: btn.danger
                ? (btn.hovered || btn.down ? Qt.darker(root.red, 1.08) : root.red)
                : (btn.primary
                   ? (btn.hovered || btn.down ? Qt.darker(root.blue, 1.08) : root.blue)
                   : (btn.hovered || btn.down ? root.fill : root.menuBg))
            border.width: uniform ? 1 : 0
            border.color: btn.activeFocus ? root.blue : root.menuBorder
        }
        contentItem: Text {
            id: txt
            text: btn.text
            color: btn.primary || btn.danger ? "#ffffff" : root.label
            font.family: root.fonts[0]
            font.pixelSize: btn.uniform ? 12 : 13
            font.weight: btn.uniform ? Font.Medium
                                     : (btn.primary || btn.danger ? Font.DemiBold : Font.Medium)
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }

    Mask {
        id: quitMask
        Card {
            cardW: 360
            implicitHeight: qCol.implicitHeight + 32
            Column {
                id: qCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.t("quit.title")
                    color: root.orange
                    font.family: root.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: root.t("quit.msg")
                    color: root.secondary
                    wrapMode: Text.WordWrap
                    font.family: root.fonts[0]
                    font.pixelSize: 13
                }
                Item { width: 1; height: 6 }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DBtn { text: root.t("btn.cancel"); onClicked: quitMask.visible = false }
                    DBtn { text: root.t("quit.confirm"); primary: true; onClicked: root.confirmQuit() }
                }
            }
        }
    }

    Mask {
        id: groupsMask
        Card {
            cardW: 500
            height: Math.min(520, Math.max(260, groupsBody.implicitHeight + 32))
            ColumnLayout {
                id: groupsBody
                anchors.fill: parent
                anchors.margins: 16
                spacing: 8
                Text {
                    text: root.t("groups.title")
                    color: root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    Layout.fillWidth: true
                }
                Text {
                    text: root.t("groups.sub")
                    color: root.secondary
                    wrapMode: Text.WordWrap
                    font.family: root.fonts[0]
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
                        radius: root.r
                        color: selected ? root.fill : (rowHit.containsMouse ? root.fill : "transparent")
                        border.width: 1
                        border.color: selected ? root.blue : root.menuBorder
                        activeFocusOnTab: true
                        Accessible.role: Accessible.ListItem
                        Accessible.name: groupRow.name + ", " + groupRow.count + " " + root.t("js.nodes")
                        readonly property bool selected: {
                            var currentHome = root.home
                            var cur = (currentHome && currentHome.activeGid) || root.gid()
                            return cur === gid
                        }
                        function activate() {
                            if (root.home && typeof root.home.switchGroup === "function")
                                root.home.switchGroup(groupRow.gid, true)
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
                                    color: root.label
                                    font.family: root.fonts[0]
                                    font.pixelSize: 13
                                    font.weight: groupRow.selected ? Font.DemiBold : Font.Medium
                                    elide: Text.ElideRight
                                }
                                Text {
                                    text: root.t("js.nodes") + " · " + groupRow.count
                                    color: root.secondary
                                    font.family: root.fonts[0]
                                    font.pixelSize: 11
                                }
                            }
                            Row {
                                z: 1
                                Layout.alignment: Qt.AlignVCenter
                                spacing: 6
                                DBtn { uniform: true; text: root.t("ctx.edit"); onClicked: root.openGroupEdit(groupRow.gid, "edit") }
                                DBtn { uniform: true; text: root.t("ctx.delete"); danger: true; onClicked: root.deleteGroup(groupRow.gid) }
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    DBtn { uniform: true; text: root.t("groups.new"); onClicked: root.openGroupEdit(null, "create") }
                    Item { Layout.fillWidth: true }
                    DBtn { uniform: true;
                        text: root.t("groups.updateAll")
                        enabled: !root.subUpdating
                        onClicked: root.updateAllGroups()
                    }
                    DBtn { uniform: true; text: root.t("groups.done"); primary: true; onClicked: groupsMask.visible = false }
                }
            }
        }
    }

    Mask {
        id: groupEditMask
        z: 410
        Card {
            cardW: 420
            implicitHeight: groupEditBody.implicitHeight + 32
            ColumnLayout {
                id: groupEditBody
                anchors.fill: parent
                anchors.margins: 16
                spacing: 8
                Text {
                    id: geTitle
                    color: root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    Layout.fillWidth: true
                }
                Text {
                    id: geSub
                    color: root.secondary
                    wrapMode: Text.WordWrap
                    font.family: root.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                Item { Layout.preferredHeight: 2 }
                Text {
                    text: root.t("edit.name")
                    color: root.secondary
                    font.family: root.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                TextField {
                    id: geName
                    Layout.fillWidth: true
                    Layout.preferredHeight: 32
                    leftPadding: 10
                    rightPadding: 10
                    placeholderText: root.t("edit.name")
                    font.family: root.fonts[0]
                    font.pixelSize: 13
                    color: root.label
                    selectByMouse: true
                    Accessible.name: root.t("edit.name")
                    Keys.onReturnPressed: root.saveGroupEdit()
                    Keys.onEnterPressed: root.saveGroupEdit()
                    onTextEdited: root.groupEditError = ""
                    background: Rectangle {
                        radius: root.r
                        color: root.menuBg
                        border.width: 1
                        border.color: geName.activeFocus ? root.blue : root.menuBorder
                    }
                }
                Text {
                    text: root.t("label.728c7d71")
                    color: root.secondary
                    font.family: root.fonts[0]
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
                    font.family: root.fonts[0]
                    font.pixelSize: 13
                    color: root.label
                    selectByMouse: true
                    inputMethodHints: Qt.ImhUrlCharactersOnly
                    Accessible.name: root.t("label.728c7d71")
                    onTextEdited: root.groupEditError = ""
                    background: Rectangle {
                        radius: root.r
                        color: root.menuBg
                        border.width: 1
                        border.color: geUrl.activeFocus ? root.blue : root.menuBorder
                    }
                }
                Text {
                    visible: !!root.groupEditError
                    text: root.groupEditError
                    color: root.red
                    wrapMode: Text.WordWrap
                    font.family: root.fonts[0]
                    font.pixelSize: 12
                    Layout.fillWidth: true
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Item { Layout.fillWidth: true }
                    DBtn { uniform: true; text: root.t("btn.cancel"); onClicked: groupEditMask.visible = false }
                    DBtn { uniform: true; text: root.t("btn.save"); primary: true; onClicked: root.saveGroupEdit() }
                }
            }
        }
    }

    Mask {
        id: statsMask
        Card {
            cardW: 440
            implicitHeight: stCol.implicitHeight + 32
            Column {
                id: stCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 10
                Text {
                    text: root.t("stats.title")
                    color: root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Grid {
                    width: parent.width
                    columns: 2
                    columnSpacing: 12
                    rowSpacing: 10
                    Repeater {
                        model: [
                            { k: "stats.core", v: "core" },
                            { k: "dock.conn", v: "conn" },
                            { k: "Proxy", v: "proxy" },
                            { k: "Direct", v: "direct" },
                            { k: "stats.uptime", v: "uptime" },
                            { k: "stats.exitIp", v: "ip" },
                            { k: "stats.country", v: "country" },
                            { k: "stats.nextSub", v: "next" }
                        ]
                        delegate: Column {
                            required property var modelData
                            width: (stCol.width - 12) / 2
                            spacing: 2
                            Text {
                                text: modelData.k.indexOf(".") >= 0 ? root.t(modelData.k) : modelData.k
                                color: root.tertiary
                                font.family: root.fonts[0]
                                font.pixelSize: 11
                            }
                            Text {
                                text: String((root.stats && root.stats[modelData.v]) || "—")
                                color: root.label
                                font.family: root.fonts[0]
                                font.pixelSize: 13
                                wrapMode: Text.Wrap
                                width: parent.width
                            }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DBtn { text: root.t("btn.refresh"); onClicked: { root.fillStats(); root.log("SYS", "info", root.t("log.runtimeRefreshed")) } }
                    DBtn { text: root.t("btn.close"); primary: true; onClicked: statsMask.visible = false }
                }
            }
        }
    }

    Mask {
        id: exportMask
        Card {
            cardW: 440
            implicitHeight: Math.min(480, exCol.implicitHeight + 32)
            Column {
                id: exCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.t("export.title")
                    color: root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: root.t("export.sub")
                    color: root.secondary
                    font.family: root.fonts[0]
                    font.pixelSize: 13
                }
                ScrollView {
                    width: parent.width
                    height: 220
                    TextArea {
                        text: root.exportText
                        readOnly: true
                        wrapMode: TextEdit.Wrap
                        color: root.label
                        font.family: root.mono[0]
                        font.pixelSize: 11
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DBtn { text: root.t("btn.close"); onClicked: exportMask.visible = false }
                    DBtn { text: root.t("btn.copyAll"); primary: true; onClicked: root.copyExport() }
                }
            }
        }
    }

    component CtxItem: AbstractButton {
        id: ctxItem
        property string act: ""
        width: parent ? parent.width : 220
        height: 28
        hoverEnabled: true
        onClicked: root.ctxAct(ctxItem.act)
        background: Rectangle {
            radius: 4
            color: ctxItem.hovered && ctxItem.enabled ? root.fill : "transparent"
        }
        contentItem: Text {
            text: ctxItem.text
            color: ctxItem.enabled ? root.label : root.tertiary
            font.family: root.fonts[0]
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
            leftPadding: 8
            opacity: ctxItem.enabled ? 1 : 0.45
        }
    }
    component CtxSep: Item {
        width: parent ? parent.width : 220
        height: 9
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 6
            height: 1
            color: th ? th.separator : "#1e3c3c43"
        }
    }

    Popup {
        id: nodeCtx
        parent: Overlay.overlay
        padding: 6
        width: 232
        height: Math.min(naturalHeight, Math.max(1, (parent ? parent.height : naturalHeight) - 16))
        modal: false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle {
            radius: 12
            color: root.menuBg
            border.color: root.menuBorder
            border.width: 1
        }
        property int selCount: 0
        property real anchorX: 0
        property real anchorY: 0
        readonly property real naturalHeight: nodeCtxColumn.implicitHeight + topPadding + bottomPadding
        function placeAt(px, py) {
            anchorX = px
            anchorY = py
            reposition()
        }
        function reposition() {
            var ov = parent
            if (!ov) { x = anchorX; y = anchorY; return }
            var margin = 8
            var nx = anchorX
            var ny = anchorY
            if (nx + width + margin > ov.width) nx = anchorX - width
            if (ny + height + margin > ov.height) ny = anchorY - height
            x = Math.max(margin, Math.min(nx, Math.max(margin, ov.width - width - margin)))
            y = Math.max(margin, Math.min(ny, Math.max(margin, ov.height - height - margin)))
        }
        onOpened: reposition()
        onHeightChanged: if (visible) reposition()
        Connections {
            target: nodeCtx.parent
            function onWidthChanged() { if (nodeCtx.visible) nodeCtx.reposition() }
        }
        readonly property bool hasSel: {
            if (selCount > 0) return true
            var t = root.home && root.home.table
            if (!t) return false
            var names = t.selectedNames
            if (names && names.length) return true
            return !!(t.selectedName && t.selectedName !== "—")
        }
        readonly property bool hasUrl: {
            var data = root.catalog || (root.home && root.home.catalog)
            if (!data || !data.groups || !data.groups.length) return false
            var id = (root.home && root.home.activeGid) || (root.win && root.win.subTab) || data.active || "default"
            var g = null
            for (var i = 0; i < data.groups.length; i++)
                if (data.groups[i].id === id) { g = data.groups[i]; break }
            if (!g) g = data.groups[0]
            return !!(g && g.url)
        }
        contentItem: Flickable {
            id: nodeCtxScroll
            clip: true
            contentWidth: width
            contentHeight: nodeCtxColumn.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick
            ScrollBar.vertical: ScrollBar {
                policy: nodeCtxScroll.contentHeight > nodeCtxScroll.height
                    ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
            }
            Column {
                id: nodeCtxColumn
                width: nodeCtxScroll.width
                spacing: 0
                CtxItem { text: root.t("ctx.addClip"); act: "add-clip" }
                CtxItem { text: root.t("ctx.addFile"); act: "add-file" }
                CtxItem { text: root.t("menu.scanQr"); act: "scan-qr" }
                CtxSep {}
                CtxItem { text: root.t("ctx.edit"); act: "edit"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.t("ctx.start"); act: "start"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.t("ctx.stop"); act: "stop"; enabled: !!(root.home && root.home.connected) }
                CtxItem { text: root.t("ctx.clone"); act: "clone"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.t("ctx.delete"); act: "delete"; enabled: nodeCtx.hasSel }
                CtxSep {}
                CtxItem { text: root.t("ctx.copyLink"); act: "copy-link"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.t("ctx.showQr"); act: "show-qr"; enabled: nodeCtx.hasSel }
                CtxSep {}
                CtxItem { text: root.t("ctx.selectAll"); act: "select-all" }
                CtxItem { text: root.t("ctx.refreshSub"); act: "refresh-sub"; enabled: nodeCtx.hasUrl }
                CtxSep {}
                CtxItem { text: root.t("test.urlSelected"); act: "url-test"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.t("ctx.resolveIp"); act: "resolve-ip"; enabled: nodeCtx.hasSel }
                CtxItem { text: root.t("test.clear"); act: "clear-test" }
                CtxItem { text: root.t("ctx.resetTraffic"); act: "reset-traffic" }
                CtxSep {}
                CtxItem { text: root.t("ctx.dedupe"); act: "dedupe" }
                CtxItem { text: root.t("ctx.rmUnavailable"); act: "rm-unavailable" }
                CtxItem { text: root.t("ctx.rmFailed"); act: "rm-failed" }
            }
        }
    }

    Mask {
        id: askMask
        z: 420
        dismiss: function () { root.closeAsk(false) }
        Card {
            cardW: 360
            implicitHeight: askCol.implicitHeight + 32
            Column {
                id: askCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.askTitle
                    color: root.askDanger ? root.orange : root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    text: root.askMsg
                    color: root.secondary
                    wrapMode: Text.WordWrap
                    font.family: root.fonts[0]
                    font.pixelSize: 13
                }
                Item { width: 1; height: 6 }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DBtn {
                        text: root.t("btn.cancel")
                        uniform: root.askUniform
                        onClicked: root.closeAsk(false)
                    }
                    DBtn {
                        text: root.askOkText
                        uniform: root.askUniform
                        primary: !root.askDanger
                        danger: root.askDanger
                        onClicked: root.closeAsk(true)
                    }
                }
            }
        }
    }

    Mask {
        id: qrMask
        Card {
            cardW: 380
            implicitHeight: qrCol.implicitHeight + 32
            Column {
                id: qrCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.t("qr.title")
                    color: root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Text {
                    text: root.qrName
                    color: root.secondary
                    font.family: root.fonts[0]
                    font.pixelSize: 13
                    elide: Text.ElideRight
                    width: parent.width
                }
                Item {
                    width: parent.width
                    height: 228
                    Image {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: root.qrSvg.length > 0
                        source: root.qrSvg.length ? ("data:image/svg+xml;utf8," + encodeURIComponent(root.qrSvg)) : ""
                        sourceSize.width: 220
                        sourceSize.height: 220
                        fillMode: Image.PreserveAspectFit
                        width: 220
                        height: 220
                    }
                    Text {
                        visible: !root.qrSvg.length
                        anchors.centerIn: parent
                        text: root.qrLink ? root.t("js.generating") : root.t("qr.empty")
                        color: root.tertiary
                        font.family: root.fonts[0]
                        font.pixelSize: 13
                    }
                }
                Text {
                    width: parent.width
                    text: root.qrLink || root.t("qr.noShare")
                    color: root.secondary
                    wrapMode: Text.WrapAnywhere
                    font.family: root.mono[0]
                    font.pixelSize: 11
                    maximumLineCount: 6
                    elide: Text.ElideRight
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DBtn { text: root.t("btn.close"); onClicked: qrMask.visible = false }
                    DBtn { text: root.t("ctx.copyLink"); primary: true; onClicked: root.copyQr() }
                }
            }
        }
    }

    Mask {
        id: editMask
        Card {
            cardW: 480
            implicitHeight: Math.min(560, edCol.implicitHeight + 32)
            Column {
                id: edCol
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 18
                spacing: 8
                Text {
                    text: root.t("edit.title")
                    color: root.label
                    font.family: root.fonts[0]
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                ScrollView {
                    width: parent.width
                    height: Math.min(420, edForm.implicitHeight + 8)
                    clip: true
                    Column {
                        id: edForm
                        width: edCol.width
                        spacing: 8
                        TextField {
                            width: parent.width
                            text: root.eName
                            placeholderText: root.t("edit.name")
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eName = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        Row {
                            width: parent.width
                            spacing: 8
                            ComboBox {
                                id: eTypeBox
                                width: (parent.width - 8) / 2
                                model: ["VLESS", "Trojan", "SS", "VMess", "HTTP", "HTTPS", "SOCKS", "AnyTLS", "TUIC"]
                                onActivated: root.eType = currentText
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.ePort
                                placeholderText: root.t("edit.port")
                                color: root.label
                                font.family: root.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.ePort = text
                                background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                            }
                        }
                        TextField {
                            width: parent.width
                            text: root.eServer
                            placeholderText: root.t("edit.server")
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eServer = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        TextField {
                            visible: root.eShow("vless vmess tuic")
                            width: parent.width
                            text: root.eUuid
                            placeholderText: "UUID"
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eUuid = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        TextField {
                            visible: root.eShow("vless")
                            width: parent.width
                            text: root.eFlow
                            placeholderText: "Flow"
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eFlow = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        Row {
                            visible: root.eShow("vmess")
                            width: parent.width
                            spacing: 8
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.eSecurity
                                placeholderText: root.t("edit.security")
                                color: root.label
                                font.family: root.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.eSecurity = text
                                background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.eAlterId
                                placeholderText: "AlterID"
                                color: root.label
                                font.family: root.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.eAlterId = text
                                background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                            }
                        }
                        TextField {
                            visible: root.eShow("http https socks")
                            width: parent.width
                            text: root.eUser
                            placeholderText: root.t("edit.user")
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eUser = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        TextField {
                            visible: root.eShow("trojan ss http https socks anytls tuic")
                            width: parent.width
                            text: root.ePass
                            placeholderText: root.t("edit.pass")
                            echoMode: TextInput.Password
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.ePass = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        TextField {
                            visible: root.eShow("ss")
                            width: parent.width
                            text: root.eMethod
                            placeholderText: root.t("edit.method")
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eMethod = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        TextField {
                            visible: root.eShow("tuic")
                            width: parent.width
                            text: root.eCongest
                            placeholderText: root.t("edit.congest")
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eCongest = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        TextField {
                            visible: root.eShow("tuic")
                            width: parent.width
                            text: root.eAlpn
                            placeholderText: "ALPN"
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eAlpn = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        TextField {
                            visible: root.eShow("vless vmess trojan https anytls tuic")
                            width: parent.width
                            text: root.eSni
                            placeholderText: root.t("edit.sni")
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eSni = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        Row {
                            visible: root.eShow("vless vmess trojan")
                            width: parent.width
                            spacing: 8
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.eNetwork
                                placeholderText: root.t("edit.network")
                                color: root.label
                                font.family: root.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.eNetwork = text
                                background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                            }
                            TextField {
                                width: (parent.width - 8) / 2
                                text: root.eHost
                                placeholderText: root.t("edit.host")
                                color: root.label
                                font.family: root.fonts[0]
                                font.pixelSize: 13
                                onTextChanged: root.eHost = text
                                background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                            }
                        }
                        TextField {
                            visible: root.eShow("vless vmess trojan http https")
                            width: parent.width
                            text: root.ePath
                            placeholderText: root.t("edit.path")
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.ePath = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                        CheckBox {
                            visible: root.eShow("vless vmess trojan https anytls")
                            text: root.t("edit.tls")
                            checked: root.eTls
                            onToggled: root.eTls = checked
                        }
                        CheckBox {
                            visible: root.eShow("vless vmess trojan https anytls tuic")
                            text: root.t("edit.insecure")
                            checked: root.eInsecure
                            onToggled: root.eInsecure = checked
                        }
                        TextField {
                            width: parent.width
                            text: root.eNote
                            placeholderText: root.t("edit.note")
                            color: root.label
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            onTextChanged: root.eNote = text
                            background: Rectangle { radius: 8; color: root.menuBg; border.width: 1; border.color: root.menuBorder }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 10
                    DBtn { text: root.t("btn.cancel"); onClicked: editMask.visible = false }
                    DBtn { text: root.t("btn.save"); primary: true; onClicked: root.saveEdit() }
                }
            }
        }
    }
}
