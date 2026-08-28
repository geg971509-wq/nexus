import QtQuick

QtObject {
    id: flow

    required property var host
    readonly property var home: host.home
    readonly property var catalog: host.catalog

    property bool creatingGroup: false
    property string editId: ""
    property string groupEditError: ""

    function t(k, v) { return host.t(k, v) }
    function loadCatalogBlob() { return host.loadCatalogBlob() }
    function putCatalog(blob) { return host.putCatalog(blob) }
    function reloadHome() { host.reloadHome() }
    function log(tag, cls, msg) { host.log(tag, cls, msg) }

    function openGroups() {
        var data = loadCatalogBlob()
        var rows = []
        var groups = (data && data.groups) ? data.groups : []
        for (var i = 0; i < groups.length; i++) {
            var g = groups[i]
            rows.push({ gid: g.id || "", name: g.name || "", count: g.count || 0 })
        }
        host.setGroupRows(rows)
        host.showGroupsDialog()
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
            host.setGroupEditForm(t("js.newGroup"), t("js.newGroupSub"), "", "")
        } else {
            if (!g) return
            host.setGroupEditForm(t("js.editGroup"), t("js.editGroupSub"), g.name || "", g.url || "")
        }
        host.showGroupEditDialog()
    }

    function saveGroupEdit() {
        var name = host.groupEditName().trim()
        groupEditError = ""
        if (!name) {
            groupEditError = t("log.groupNameEmpty")
            host.focusGroupEditName()
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
                host.focusGroupEditName()
                return
            }
        }
        var url = host.groupEditUrl().trim()
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
        host.hideGroupEditDialog()
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
            host.askConfirm(t("confirm.deleteGroupLive", { name: g.name, node: live }), {
                title: t("confirm.deleteGroupTitle"),
                okText: t("btn.ok"),
                uniform: true
            }, function () {})
            log("SYS", "warn", t("log.groupLiveInUse", { name: g.name, node: live }))
            return
        }
        host.askConfirm(t("confirm.deleteGroup", { name: g.name }), {
            title: t("confirm.deleteGroupTitle"),
            okText: t("ctx.delete"),
            danger: true,
            uniform: true
        }, function (ok) { if (ok) flow.deleteGroupConfirmed(id) })
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
}
