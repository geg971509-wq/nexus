import QtQuick

QtObject {
    id: root
    property bool dark: false
    property string fontChoice: "系统默认"
    property string iconStyle: "Monochrome"

    readonly property var fontFamilies: fontChoice === "PingFang SC"
        ? ["PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "SF Pro Text", "Helvetica Neue"]
        : (fontChoice === "SF Pro"
           ? ["SF Pro Text", "SF Pro Display", "Helvetica Neue", "PingFang SC"]
           : ["SF Pro Text", "SF Pro Display", "PingFang SC", "Hiragino Sans GB",
              "Microsoft YaHei", "Helvetica Neue", "Segoe UI"])
    readonly property var monoFamilies: ["SF Mono", "Menlo", "Monaco", "Consolas", "ui-monospace"]

    readonly property color windowBackground: dark ? "#1f1f22" : "#f5f5f7"
    readonly property color windowBackgroundMacOS: dark
        ? Qt.rgba(31 / 255, 31 / 255, 34 / 255, 0.85)
        : Qt.rgba(245 / 255, 245 / 255, 247 / 255, 0.85)
    readonly property color bg: Qt.platform.os === "osx" ? windowBackgroundMacOS : windowBackground
    readonly property color surface: dark ? "#2c2c2e" : "#ffffff"
    readonly property color surfaceElevated: dark ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(1, 1, 1, 0.88)
    readonly property color groupedBackground: dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(1, 1, 1, 0.74)
    readonly property color rowSurface: dark ? Qt.rgba(1, 1, 1, 0.035) : Qt.rgba(1, 1, 1, 0.62)
    readonly property color fill: dark ? Qt.rgba(1, 1, 1, 0.12) : Qt.rgba(0, 0, 0, 0.085)
    readonly property color fill2: dark ? Qt.rgba(1, 1, 1, 0.04) : Qt.rgba(0, 0, 0, 0.04)
    readonly property color separator: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0, 0, 0, 0.08)
    readonly property color label: dark ? "#f5f5f7" : "#1d1d1f"
    readonly property color secondary: dark ? "#a1a1a6" : "#6e6e73"
    readonly property color tertiary: dark ? "#8e8e93" : "#8e8e93"
    readonly property color quaternary: dark ? "#636366" : "#aeaeb2"
    readonly property color blue: "#0a84ff"
    readonly property color selectionSoft: Qt.rgba(10 / 255, 132 / 255, 255 / 255, dark ? 0.22 : 0.16)
    readonly property color selectionHover: Qt.rgba(10 / 255, 132 / 255, 255 / 255, dark ? 0.28 : 0.20)
    readonly property color selectionStroke: Qt.rgba(10 / 255, 132 / 255, 255 / 255, dark ? 0.34 : 0.28)
    readonly property color blueSoft: selectionSoft
    readonly property color green: dark ? "#30d158" : "#34c759"
    readonly property color greenSoft: dark ? "#3330d158" : "#2434c759"
    readonly property color orange: dark ? "#ff9f0a" : "#ff9500"
    readonly property color red: dark ? "#ff453a" : "#ff3b30"
    readonly property color purple: dark ? "#bf5af2" : "#af52de"
    readonly property color icon: dark ? "#d1d1d6" : "#3a3a3c"
    readonly property color pressed: fill
    readonly property color chrome: bg
    readonly property color chromeSolid: bg
    readonly property color hairline: separator
    readonly property color controlBg: dark ? "#2c2c2e" : "#ffffff"
    readonly property color controlText: dark ? "#f5f5f7" : "#000000"
    readonly property color controlStroke: dark ? Qt.rgba(1, 1, 1, 0.13) : Qt.rgba(0, 0, 0, 0.10)
    readonly property color menuBg: controlBg
    readonly property color menuBorder: controlStroke
    readonly property color heroTop: bg
    readonly property color heroBot: bg
    readonly property color heroBorder: separator
    readonly property color tableBorder: separator
    readonly property color sideHover: dark ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(0, 0, 0, 0.055)
    readonly property color sideActive: selectionSoft
    readonly property color sidebarBackground: Qt.platform.os === "osx"
        ? "transparent"
        : (dark ? "#1c1c1e" : "#f5f5f7")
    readonly property color sidebarTop: sidebarBackground
    readonly property color sidebarBot: sidebarBackground
    readonly property color rowHover: rowSurface
    readonly property color rowSelected: selectionSoft
    readonly property color rowSelectedHover: selectionHover
    readonly property color rowConnected: Qt.rgba(48 / 255, 209 / 255, 88 / 255, dark ? 0.16 : 0.12)
    readonly property color rowConnectedHover: Qt.rgba(48 / 255, 209 / 255, 88 / 255, dark ? 0.24 : 0.18)
    readonly property color rowConnectedSelected: Qt.rgba(48 / 255, 209 / 255, 88 / 255, dark ? 0.22 : 0.16)
    readonly property color badgeStroke: Qt.rgba(10 / 255, 132 / 255, 255 / 255, dark ? 0.22 : 0.16)
    readonly property color flowUpload: blue
    readonly property color flowDownload: purple
    readonly property color knob: "#ffffff"
    readonly property color latGood: dark ? "#32d74b" : "#248a3d"
    readonly property color latMid: dark ? "#ff9f0a" : "#c93400"
    readonly property color latBad: dark ? "#ff453a" : "#d70015"
    readonly property color switchTrack: dark ? "#454545" : "#d9d6d2"
    readonly property int radius: 8
    readonly property int radiusLg: 10
    readonly property int sidebarW: 200
    readonly property int sidebarCollapsedW: 72
    readonly property int sideItemH: 36
    readonly property int titleChromeH: 28
    readonly property int titleToolsH: 36
    readonly property int statusH: 24
    readonly property int dockCollapsedH: 28
    readonly property int fastAnimation: 160
    readonly property int mediumAnimation: 220
    readonly property real ease: 0.15

    readonly property Palette palette: Palette {
        window: root.bg
        windowText: root.label
        base: root.controlBg
        alternateBase: root.groupedBackground
        text: root.label
        button: root.controlBg
        buttonText: root.label
        brightText: "#ffffff"
        highlight: root.blue
        highlightedText: "#ffffff"
        placeholderText: root.secondary
        accent: root.blue
        link: root.blue
        linkVisited: root.blue
        toolTipBase: root.surfaceElevated
        toolTipText: root.label
        light: root.dark ? root.controlStroke : "#ffffff"
        midlight: root.controlStroke
        mid: root.separator
        dark: root.dark ? "#111113" : "#8e8e93"
        shadow: root.dark ? "#000000" : Qt.rgba(0, 0, 0, 0.35)
    }
}
