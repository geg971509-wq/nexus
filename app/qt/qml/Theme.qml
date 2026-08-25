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

    readonly property color bg: dark ? "#161618" : "#f5f5f7"
    readonly property color surface: dark ? "#2a2a2c" : "#ffffff"
    readonly property color surfaceElevated: dark ? "#363638" : "#ffffff"
    readonly property color fill: dark ? "#3898989d" : "#1e787880"
    readonly property color fill2: dark ? "#2898989d" : "#14787880"
    readonly property color separator: dark ? "#a6545458" : "#1e3c3c43"
    readonly property color label: dark ? "#f5f5f7" : "#1d1d1f"
    readonly property color secondary: dark ? "#98989d" : "#6e6e73"
    readonly property color tertiary: dark ? "#8e8e93" : "#8e8e93"
    readonly property color quaternary: dark ? "#636366" : "#aeaeb2"
    readonly property color blue: dark ? "#0a84ff" : "#007aff"
    readonly property color blueSoft: dark ? "#380a84ff" : "#1e007aff"
    readonly property color green: dark ? "#30d158" : "#34c759"
    readonly property color greenSoft: dark ? "#3330d158" : "#2434c759"
    readonly property color orange: "#ff9f0a"
    readonly property color red: dark ? "#ff453a" : "#ff3b30"
    readonly property color purple: dark ? "#bf5af2" : "#af52de"
    readonly property color chrome: dark ? "#f0242426" : "#f5fafafc"
    readonly property color chromeSolid: dark ? "#242426" : "#fafafc"
    readonly property color hairline: dark ? "#0fffffff" : "#0b000000"
    readonly property color controlBg: dark ? "#3a3a3c" : "#ffffff"
    readonly property color controlText: dark ? "#f5f5f7" : "#000000"
    readonly property color menuBg: dark ? "#2f2f31" : "#ffffff"
    readonly property color menuBorder: dark ? "#1fffffff" : "#1a000000"
    readonly property color heroTop: dark ? "#323234" : "#ffffff"
    readonly property color heroBot: dark ? "#2a2a2c" : "#fbfbfd"
    readonly property color heroBorder: dark ? "#14ffffff" : "#0b000000"
    readonly property color tableBorder: dark ? "#0fffffff" : "#0b000000"
    readonly property color sideHover: dark ? "#14ffffff" : "#12000000"
    readonly property color sideActive: dark ? "#0a84ff" : "#007aff"
    readonly property color sidebarTop: dark ? "#2c2c2e" : "#ececef"
    readonly property color sidebarBot: dark ? "#1c1c1e" : "#e5e5ea"
    readonly property color knob: "#ffffff"
    readonly property color latGood: dark ? "#32d74b" : "#248a3d"
    readonly property color latMid: dark ? "#ff9f0a" : "#c93400"
    readonly property color latBad: dark ? "#ff453a" : "#d70015"
    readonly property color switchTrack: dark ? "#5198989d" : "#51787880"
    readonly property int radius: 6
    readonly property int radiusLg: 8
    readonly property int sidebarW: 180
    readonly property int sidebarCollapsedW: 72
    readonly property int sideItemH: 32
    readonly property int titleChromeH: 28
    readonly property int titleToolsH: 36
    readonly property int statusH: 24
    readonly property int dockCollapsedH: 28
    readonly property real ease: 0.15
}
