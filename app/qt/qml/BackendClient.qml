pragma ComponentBehavior: Bound

import QtQuick

QtObject {
    id: client

    required property var bridge

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
        var api = bridge
        if (!api || typeof api.invoke !== "function")
            return { ok: false, offline: true }
        try {
            var json = payload == null ? "{}" : (typeof payload === "string" ? payload : JSON.stringify(payload))
            return parseReply(api.invoke(cmd, json))
        } catch (e) {
            return { ok: false, error: String(e) }
        }
    }

    function unwrapCatalog(blob) {
        if (!blob || typeof blob !== "object") return null
        if (blob.v === 1 && blob.groups) return blob
        if (blob.data && blob.data.v === 1) return blob.data
        if (blob.catalog && blob.catalog.v === 1) return blob.catalog
        return null
    }
}
