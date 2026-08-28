pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtCore

Item {
    id: root
    readonly property var win: Window.window
    readonly property var th: win ? win.theme : null
    readonly property var i18: win ? win.i18n : null
    readonly property var fonts: th ? th.fontFamilies : ["PingFang SC"]
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
    readonly property color knob: th ? th.knob : "#ffffff"
    readonly property color switchTrack: th ? th.switchTrack : "#51787880"
    readonly property color controlBg: th ? th.controlBg : "#ffffff"
    readonly property color controlText: th ? th.controlText : "#000000"
    readonly property color menuBg: th ? th.menuBg : "#ffffff"
    readonly property color menuBorder: th ? th.menuBorder : "#1a000000"
    readonly property color rowHover: th ? th.sideHover : "#09000000"
    readonly property bool dark: th ? th.dark : false
    readonly property int rLg: th ? th.radiusLg : 14
    readonly property color sectionLabel: dark ? Qt.rgba(0.922, 0.922, 0.961, 0.48)
                                               : Qt.rgba(0.235, 0.235, 0.263, 0.58)
    readonly property color controlBorder: dark ? Qt.rgba(1, 1, 1, 0.12) : Qt.rgba(0, 0, 0, 0.14)
    readonly property color controlBorderHover: dark ? Qt.rgba(1, 1, 1, 0.26) : Qt.rgba(0, 0, 0, 0.28)
    readonly property color footBg: dark ? "#f01c1c1e" : "#ebfafafc"
    readonly property color dirtyBg: dark ? "#24ff9f0a" : "#1aff9500"
    readonly property color dirtyBd: dark ? "#52ff9f0a" : "#47ff9500"
    readonly property color dirtyFg: dark ? "#ffd60a" : "#9a5b00"
    readonly property bool isSub: win && win.currentView === "sub"
    readonly property string lang: i18 ? i18.lang : "zh-CN"

    readonly property var langOpts: ["简体中文", "English", "Русский", "繁體中文"]
    readonly property var fontOpts: ["系统默认", "SF Pro", "PingFang SC"]
    readonly property var themeOpts: ["System", "浅色", "深色"]
    readonly property var iconOpts: ["Monochrome", "Colorful", "Auto"]
    readonly property var langMap: ({ "简体中文": "zh-CN", "English": "en", "Русский": "ru", "繁體中文": "zh-TW" })

    property string draftLang: "简体中文"
    property string draftFont: "系统默认"
    property string draftTheme: "System"
    property string draftIcon: "Monochrome"
    property bool draftHide: false
    property string savedLang: "简体中文"
    property string savedFont: "系统默认"
    property string savedTheme: "System"
    property string savedIcon: "Monochrome"
    property bool savedHide: false
    property bool dirty: false
    property bool applying: false
    readonly property bool subBusy: !!(win && win.dialogs && win.dialogs.subUpdating)
    property string subUrl: ""
    property string groupName: "Default"
    property string groupId: "default"
    property var catalog: null

    Settings {
        id: store
        category: "ui"
        property string lang: "简体中文"
        property string font: "系统默认"
        property string theme: "System"
        property string icon: "Monochrome"
    }

    function t(k, v) {
        var _ = lang
        return i18 ? i18.t(k, v) : k
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

    function pick(v, opts, fallback) {
        return opts.indexOf(v) >= 0 ? v : fallback
    }

    function optLabel(canonical) {
        if (canonical === "系统默认") return t("opt.font.system")
        if (canonical === "System") return t("opt.theme.system")
        if (canonical === "浅色") return t("opt.theme.light")
        if (canonical === "深色") return t("opt.theme.dark")
        return canonical
    }

    function osDark() {
        return Qt.styleHints.colorScheme === Qt.ColorScheme.Dark
    }

    function applyLocale(label) {
        if (!i18) return
        i18.lang = langMap[label] || "zh-CN"
        var a = api()
        if (a && typeof a.setTrayLabels === "function")
            a.setTrayLabels(i18.t("tray.showWindow"), i18.t("menu.quit"))
    }

    function applyFont(label) {
        if (th) th.fontChoice = label
    }

    function applyTheme(label) {
        if (!th) return
        if (label === "深色") th.dark = true
        else if (label === "浅色") th.dark = false
        else th.dark = osDark()
    }

    function applyIcon(label) {
        if (th) th.iconStyle = (label === "Colorful") ? "Colorful" : "Monochrome"
    }

    function applyAll(lang, font, theme, icon) {
        applying = true
        applyLocale(lang)
        applyFont(font)
        applyTheme(theme)
        applyIcon(icon)
        applying = false
    }

    function markDirty() {
        dirty = draftLang !== savedLang || draftFont !== savedFont
                || draftTheme !== savedTheme || draftIcon !== savedIcon
                || draftHide !== savedHide
    }

    function setHide(on, live) {
        draftHide = !!on
        if (live) {
            var r = invoke("set_hide_tray", { hide: !!on })
            if (r && r.ok === false) {
                draftHide = !on
                hideSw.on = !on
            }
        }
        markDirty()
    }

    function loadHide() {
        var r = invoke("store_snapshot", {})
        var d = (r && r.ok) ? (r.data || r) : {}
        var on = !!(d && d.hide_tray)
        savedHide = on
        draftHide = on
        hideSw.on = on
    }

    function unwrapCatalog(blob) {
        if (!blob || typeof blob !== "object") return null
        if (blob.v === 1 && blob.groups) return blob
        if (blob.data && blob.data.v === 1) return blob.data
        if (blob.catalog && blob.catalog.v === 1) return blob.catalog
        return null
    }

    function homeSibling() {
        var p = parent
        if (!p) return null
        var kids = p.children
        for (var i = 0; i < kids.length; i++) {
            if (kids[i] !== root && typeof kids[i].loadCatalog === "function")
                return kids[i]
        }
        return null
    }

    function activeGroup() {
        var data = catalog
        if (!data || !data.groups || !data.groups.length)
            return { id: "default", name: "Default", url: "" }
        var gid = data.active
        var home = homeSibling()
        if (home && home.activeGid) gid = home.activeGid
        var g = null
        for (var i = 0; i < data.groups.length; i++) {
            if (data.groups[i].id === gid) { g = data.groups[i]; break }
        }
        return g || data.groups[0]
    }

    function loadCatalog() {
        var r = invoke("catalog_get", {})
        catalog = unwrapCatalog(r && r.ok ? (r.data || r) : (r && r.data))
        var g = activeGroup()
        groupId = g.id || "default"
        groupName = g.name || "Default"
        subUrl = g.url || ""
        urlField.text = subUrl
    }

    function putCatalog(blob) {
        return invoke("catalog_put", { blob: blob })
    }

    function loadPrefs() {
        savedLang = pick(store.lang, langOpts, "简体中文")
        savedFont = pick(store.font, fontOpts, "系统默认")
        savedTheme = pick(store.theme, themeOpts, "System")
        savedIcon = pick(store.icon, iconOpts, "Monochrome")
        draftLang = savedLang
        draftFont = savedFont
        draftTheme = savedTheme
        draftIcon = savedIcon
        applyAll(savedLang, savedFont, savedTheme, savedIcon)
        loadHide()
        dirty = false
    }

    function savePrefs() {
        store.lang = draftLang
        store.font = draftFont
        store.theme = draftTheme
        store.icon = draftIcon
        savedLang = draftLang
        savedFont = draftFont
        savedTheme = draftTheme
        savedIcon = draftIcon
        savedHide = draftHide
        dirty = false
    }

    function discardPrefs() {
        draftLang = savedLang
        draftFont = savedFont
        draftTheme = savedTheme
        draftIcon = savedIcon
        applyAll(savedLang, savedFont, savedTheme, savedIcon)
        if (draftHide !== savedHide)
            setHide(savedHide, true)
        draftHide = savedHide
        hideSw.on = savedHide
        dirty = false
    }

    function goHome() {
        if (win) win.currentView = "home"
    }

    function applySub() {
        if (subBusy) return
        var url = (urlField.text || "").trim()
        if (url === "https://…/sub") url = ""
        if (url && !/^https?:\/\//i.test(url) && !/^([a-z][a-z0-9+\-.]*):\/\//i.test(url)) {
            urlField.forceActiveFocus()
            return
        }
        var r0 = invoke("catalog_get", {})
        var data = unwrapCatalog(r0 && r0.ok ? (r0.data || r0) : (r0 && r0.data))
        if (!data || !data.groups || !data.groups.length) {
            catalog = data
            return
        }
        catalog = data
        var g = activeGroup()
        if (!g) return
        g.url = url
        groupId = g.id
        groupName = g.name || "Default"
        subUrl = url
        putCatalog(data)
        if (!url) return
        if (win && win.dialogs && typeof win.dialogs.refreshSub === "function")
            win.dialogs.refreshSub()
    }

    function onLangPicked(v) {
        draftLang = v
        applyLocale(v)
        markDirty()
    }
    function onFontPicked(v) {
        draftFont = v
        applyFont(v)
        markDirty()
    }
    function onThemePicked(v) {
        draftTheme = v
        applyTheme(v)
        markDirty()
    }
    function onIconPicked(v) {
        draftIcon = v
        applyIcon(v)
        markDirty()
    }

    Connections {
        target: Qt.styleHints
        function onColorSchemeChanged() {
            if (root.draftTheme === "System")
                root.applyTheme("System")
        }
    }

    onVisibleChanged: if (visible) {
        loadHide()
        loadCatalog()
    }
    Component.onCompleted: {
        loadPrefs()
        loadCatalog()
    }

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
                text: root.t(root.isSub ? "panel.sub" : "panel.basic")
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
            height: parent.height - head.height - footCol.height
            clip: true
            contentWidth: width
            contentHeight: rail.height + 32
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
                        visible: !root.isSub
                        width: parent.width
                        height: basicCol.height
                        radius: root.rLg
                        color: root.surface
                        border.width: 1
                        border.color: root.hairline
                        clip: false

                        Column {
                            id: basicCol
                            width: parent.width
                            Text {
                                width: parent.width
                                height: 32
                                text: root.t("sec.ui")
                                color: root.sectionLabel
                                font.family: root.fonts[0]
                                font.pixelSize: 11
                                font.weight: Font.Medium
                                leftPadding: 16
                                topPadding: 10
                                verticalAlignment: Text.AlignVCenter
                            }
                            SetRow {
                                labelKey: "label.lang"
                                hintKey: "hint.lang"
                                SetSelect {
                                    width: parent.width
                                    value: root.draftLang
                                    options: root.langOpts
                                    onPicked: function (v) { root.onLangPicked(v) }
                                }
                            }
                            SetRow {
                                labelKey: "label.font"
                                hintKey: "hint.font"
                                SetSelect {
                                    width: parent.width
                                    value: root.draftFont
                                    options: root.fontOpts
                                    localize: true
                                    onPicked: function (v) { root.onFontPicked(v) }
                                }
                            }
                            SetRow {
                                labelKey: "label.theme"
                                hintKey: "hint.theme"
                                SetSelect {
                                    width: parent.width
                                    value: root.draftTheme
                                    options: root.themeOpts
                                    localize: true
                                    onPicked: function (v) { root.onThemePicked(v) }
                                }
                            }
                            SetRow {
                                labelKey: "label.icons"
                                hintKey: "hint.icons"
                                SetSelect {
                                    width: parent.width
                                    value: root.draftIcon
                                    options: root.iconOpts
                                    onPicked: function (v) { root.onIconPicked(v) }
                                }
                            }
                            SetRow {
                                labelKey: "label.hideTray"
                                hintKey: "hint.hideTray"
                                last: true
                                SetSwitch {
                                    id: hideSw
                                    Accessible.name: root.t("label.hideTray")
                                    onFlipped: root.setHide(hideSw.on, true)
                                }
                            }
                        }
                    }

                    Rectangle {
                        visible: !root.isSub
                        width: parent.width
                        height: aboutCol.height
                        radius: root.rLg
                        color: root.surface
                        border.width: 1
                        border.color: root.hairline

                        Column {
                            id: aboutCol
                            width: parent.width
                            Text {
                                width: parent.width
                                height: 32
                                text: root.t("sec.about")
                                color: root.sectionLabel
                                font.family: root.fonts[0]
                                font.pixelSize: 11
                                font.weight: Font.Medium
                                leftPadding: 16
                                topPadding: 10
                                verticalAlignment: Text.AlignVCenter
                            }
                            SetRow {
                                labelKey: "label.openSource"
                                hintKey: "hint.openSource"
                                last: true
                            }
                        }
                    }

                    Rectangle {
                        visible: root.isSub
                        width: parent.width
                        height: subCol.height
                        radius: root.rLg
                        color: root.surface
                        border.width: 1
                        border.color: root.hairline

                        Column {
                            id: subCol
                            width: parent.width
                            Text {
                                width: parent.width
                                height: 32
                                text: root.t("sec.0c7d604b")
                                color: root.sectionLabel
                                font.family: root.fonts[0]
                                font.pixelSize: 11
                                font.weight: Font.Medium
                                leftPadding: 16
                                topPadding: 10
                                verticalAlignment: Text.AlignVCenter
                            }
                            SetRow {
                                labelKey: "label.728c7d71"
                                hintText: root.t("hint.subUrlNamed", { name: root.groupName })
                                last: true
                                wide: true
                                Row {
                                    width: parent.width
                                    spacing: 8
                                    Item {
                                        width: parent.width - applyBtn.width - 8
                                        height: 32
                                        TextField {
                                            id: urlField
                                            anchors.fill: parent
                                            z: 0
                                            placeholderText: "https://…/sub"
                                            color: root.controlText
                                            placeholderTextColor: root.tertiary
                                            font.family: root.fonts[0]
                                            font.pixelSize: 13
                                            font.weight: Font.Medium
                                            selectByMouse: true
                                            inputMethodHints: Qt.ImhUrlCharactersOnly
                                            echoMode: TextInput.Normal
                                            clip: true
                                            leftPadding: 11
                                            rightPadding: 11
                                            verticalAlignment: Text.AlignVCenter
                                            background: Rectangle {
                                                radius: 8
                                                color: root.controlBg
                                                border.width: 1
                                                border.color: urlField.activeFocus ? root.blue : (urlHover.hovered ? root.controlBorderHover : root.controlBorder)
                                            }
                                            Keys.onReturnPressed: root.applySub()
                                            Keys.onEnterPressed: root.applySub()
                                            Accessible.name: root.t("label.728c7d71")
                                        }
                                        Rectangle {
                                            anchors.fill: parent
                                            visible: !urlField.activeFocus
                                            z: 1
                                            radius: 8
                                            color: root.controlBg
                                            border.width: 1
                                            border.color: urlHover.hovered ? root.controlBorderHover : root.controlBorder
                                            HoverHandler { id: urlHover }
                                            Text {
                                                anchors.left: parent.left
                                                anchors.right: parent.right
                                                anchors.verticalCenter: parent.verticalCenter
                                                anchors.leftMargin: 11
                                                anchors.rightMargin: 11
                                                text: urlField.text.length ? urlField.text : "https://…/sub"
                                                color: urlField.text.length ? root.controlText : root.tertiary
                                                font.family: root.fonts[0]
                                                font.pixelSize: 13
                                                font.weight: urlField.text.length ? Font.Medium : Font.Normal
                                                elide: Text.ElideRight
                                                wrapMode: Text.NoWrap
                                            }
                                            MouseArea {
                                                anchors.fill: parent
                                                cursorShape: Qt.IBeamCursor
                                                onClicked: urlField.forceActiveFocus()
                                            }
                                        }
                                    }
                                    SetMini {
                                        id: applyBtn
                                        text: root.t("btn.apply")
                                        enabled: !root.subBusy
                                        Accessible.name: root.t("title.applySub")
                                        onClicked: root.applySub()
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Column {
            id: footCol
            width: parent.width

            Rectangle {
                visible: root.dirty
                width: parent.width
                height: 36
                color: root.dirtyBg
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 1
                    color: root.dirtyBd
                }
                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 24
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.t("settings.dirty")
                    color: root.dirtyFg
                    font.family: root.fonts[0]
                    font.pixelSize: 12
                    font.weight: Font.Medium
                }
                AbstractButton {
                    id: discardBtn
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    height: 26
                    implicitWidth: Math.max(48, discardTxt.implicitWidth + 20)
                    hoverEnabled: true
                    Accessible.name: root.t("settings.discard")
                    onClicked: root.discardPrefs()
                    background: Rectangle {
                        radius: 6
                        color: discardBtn.hovered ? Qt.rgba(0, 122 / 255, 1, 0.1) : "transparent"
                    }
                    contentItem: Text {
                        id: discardTxt
                        text: root.t("settings.discard")
                        color: root.blue
                        font.family: root.fonts[0]
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }

            Rectangle {
                width: parent.width
                height: 64
                color: root.footBg
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 1
                    color: root.sep
                }
                Row {
                    anchors.right: parent.right
                    anchors.rightMargin: 24
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 10
                    SetBtn {
                        text: root.t("settings.cancel")
                        primary: false
                        onClicked: {
                            if (root.dirty) root.discardPrefs()
                            root.goHome()
                        }
                    }
                    SetBtn {
                        text: root.t("settings.ok")
                        primary: true
                        onClicked: {
                            root.savePrefs()
                            root.goHome()
                        }
                    }
                }
            }
        }
    }

    component SetRow: Item {
        id: row
        property string labelKey: ""
        property string hintKey: ""
        property string hintText: ""
        property bool last: false
        property bool wide: false
        default property alias extras: rightCol.data
        width: parent ? parent.width : 0
        height: Math.max(44, inner.implicitHeight + 24)

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
        Item {
            id: inner
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: 16
            anchors.rightMargin: 16
            implicitHeight: Math.max(lab.implicitHeight, rightCol.implicitHeight)
            height: implicitHeight
            Text {
                id: lab
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.topMargin: Math.max(0, (rightCol.implicitHeight - implicitHeight) / 2)
                width: 140
                text: root.t(row.labelKey)
                color: root.label
                font.family: root.fonts[0]
                font.pixelSize: 13
                font.weight: Font.Medium
                wrapMode: Text.Wrap
            }
            Column {
                id: rightCol
                anchors.right: parent.right
                anchors.top: parent.top
                width: Math.min(row.wide ? parent.width - 156 : 340, parent.width - 156)
                spacing: 5
                Text {
                    visible: (row.hintText || row.hintKey).length > 0
                    width: parent.width
                    text: row.hintText.length ? row.hintText : root.t(row.hintKey)
                    color: root.tertiary
                    font.family: root.fonts[0]
                    font.pixelSize: 11
                    wrapMode: Text.Wrap
                    lineHeight: 1.35
                    lineHeightMode: Text.ProportionalHeight
                }
            }
        }
    }

    component SetSelect: AbstractButton {
        id: sel
        property string value: ""
        property var options: []
        property bool localize: false
        signal picked(string v)
        height: 32
        implicitHeight: 32
        implicitWidth: parent ? parent.width : 180
        hoverEnabled: true
        Accessible.name: sel.localize ? root.optLabel(sel.value) : sel.value
        onClicked: pop.open()
        background: Rectangle {
            radius: 8
            color: root.controlBg
            border.width: 1
            border.color: sel.hovered || pop.visible ? root.controlBorderHover : root.controlBorder
        }
        contentItem: Item {
            Text {
                anchors.left: parent.left
                anchors.right: chev.left
                anchors.leftMargin: 11
                anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                text: sel.localize ? root.optLabel(sel.value) : sel.value
                color: root.controlText
                font.family: root.fonts[0]
                font.pixelSize: 13
                font.weight: Font.Medium
                elide: Text.ElideRight
            }
            Text {
                id: chev
                anchors.right: parent.right
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                text: "▾"
                color: root.tertiary
                font.pixelSize: 11
            }
        }
        Popup {
            id: pop
            y: sel.height + 4
            width: Math.max(sel.width, 120)
            padding: 4
            modal: false
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
            background: Rectangle {
                radius: 10
                color: root.menuBg
                border.color: root.menuBorder
                border.width: 1
            }
            contentItem: Column {
                spacing: 1
                Repeater {
                    model: sel.options
                    delegate: AbstractButton {
                        id: optBtn
                        required property string modelData
                        width: pop.width - 8
                        height: 28
                        hoverEnabled: true
                        onClicked: {
                            sel.picked(optBtn.modelData)
                            pop.close()
                        }
                        background: Rectangle {
                            radius: 6
                            color: optBtn.hovered ? root.blue : "transparent"
                        }
                        contentItem: Text {
                            text: sel.localize ? root.optLabel(optBtn.modelData) : optBtn.modelData
                            color: optBtn.hovered ? "#ffffff" : (optBtn.modelData === sel.value ? root.blue : root.label)
                            font.family: root.fonts[0]
                            font.pixelSize: 13
                            font.weight: optBtn.modelData === sel.value ? Font.DemiBold : Font.Medium
                            leftPadding: 10
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                }
            }
        }
    }

    component SetSwitch: AbstractButton {
        id: sw
        property bool on: false
        implicitWidth: 38
        implicitHeight: 22
        width: 38
        height: 22
        hoverEnabled: true
        Accessible.role: Accessible.CheckBox
        Accessible.checkable: true
        Accessible.checked: on
        onClicked: {
            on = !on
            flipped()
        }
        signal flipped()
        background: Rectangle {
            radius: 999
            color: sw.on ? root.blue : root.switchTrack
            Behavior on color { ColorAnimation { duration: 160 } }
            Rectangle {
                width: 18
                height: 18
                radius: 9
                color: root.knob
                y: 2
                x: sw.on ? 18 : 2
                Behavior on x { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            }
        }
    }

    component SetMini: AbstractButton {
        id: mini
        height: 32
        implicitHeight: 32
        implicitWidth: Math.max(52, miniTxt.implicitWidth + 22)
        hoverEnabled: true
        opacity: enabled ? 1 : 0.45
        background: Rectangle {
            radius: 8
            color: mini.hovered && mini.enabled ? "#2e787880" : root.fill
        }
        contentItem: Text {
            id: miniTxt
            text: mini.text
            color: root.blue
            font.family: root.fonts[0]
            font.pixelSize: 12
            font.weight: Font.DemiBold
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }

    component SetBtn: AbstractButton {
        id: btn
        property bool primary: false
        height: 30
        implicitHeight: 30
        implicitWidth: Math.max(72, btnTxt.implicitWidth + 32)
        hoverEnabled: true
        background: Rectangle {
            radius: 8
            color: btn.primary
                   ? (btn.hovered ? "#0071eb" : "#007aff")
                   : (btn.hovered ? "#2e787880" : root.fill)
        }
        contentItem: Text {
            id: btnTxt
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
