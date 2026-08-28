pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window

ApplicationWindow {
    id: win
    title: "Nexus"
    width: 1100
    height: 720
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    color: theme.bg
    flags: Qt.Window

    property alias theme: theme
    property alias i18n: i18n
    property alias home: homeView
    property alias dialogs: dialogs
    property alias settings: settingsView
    property alias subscription: subscriptionView
    property string currentView: "home"
    property string subTab: "default"
    property bool sidebarCollapsed: false
    property int sidebarWidth: 180
    property int mixedPort: 2080
    property string mixedListen: "127.0.0.1:2080"
    property string sbStatus: i18n.t("sb.stopped")
    property string sbProxy: "—"
    property string sbDirect: "—"

    Theme { id: theme }
    I18n { id: i18n }

    Item {
        anchors.fill: parent

        Sidebar {
            id: sidebar
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
        }

        Item {
            anchors.left: sidebar.right
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom

            Statusbar {
                id: statusbar
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
            }

            Item {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: statusbar.top
                HomeView {
                    id: homeView
                    anchors.fill: parent
                    visible: win.currentView === "home"
                    enabled: visible
                }
                FirewallView {
                    anchors.fill: parent
                    visible: win.currentView === "firewall"
                    enabled: visible
                }
                SettingsView {
                    id: settingsView
                    anchors.fill: parent
                    visible: win.currentView === "settings"
                    enabled: visible
                }
                SubscriptionView {
                    id: subscriptionView
                    anchors.fill: parent
                    visible: win.currentView === "sub"
                    enabled: visible
                }
            }
        }
    }

    Dialogs {
        id: dialogs
    }

    Component.onCompleted: bootIdentity()

    function bootIdentity() {
        mixedPort = 2080
        mixedListen = "127.0.0.1:2080"
        // nexus is an intentionally injected C++ context property.
        // qmllint disable unqualified
        var api = (typeof nexus === "undefined") ? null : nexus
        // qmllint enable unqualified
        if (!api || typeof api.invoke !== "function")
            return
        try {
            var raw = api.invoke("app_identity", "{}")
            var obj = raw
            if (typeof raw === "string")
                obj = JSON.parse(raw)
            var p = obj && obj.mixed_port
            if (p) {
                mixedPort = Number(p)
                mixedListen = "127.0.0.1:" + mixedPort
            }
        } catch (e) { /* stay on 2080 */ }
    }
}
