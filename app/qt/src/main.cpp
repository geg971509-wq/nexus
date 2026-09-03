#include "bridge.h"
#include "host.h"
#include "nexus_c.h"
#include "tray.h"

#include <QApplication>
#include <QEvent>
#include <QJsonDocument>
#include <QJsonObject>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlError>
#include <QWindow>
#include <atomic>
#include <cstdio>

// Rust invokes these callbacks from worker threads. Atomic publication removes
// the C++ data race on the callback targets; the objects themselves outlive the
// backend teardown below, so a callback that was already in flight still points
// at a valid QObject.
static std::atomic<Tray *> g_tray{nullptr};
static std::atomic<NexusBridge *> g_bridge{nullptr};

extern "C" {
static void on_event(const char *name, const char *json) {
    NexusBridge *bridge = g_bridge.load(std::memory_order_acquire);
    if (!bridge) {
        return;
    }
    const QString n = QString::fromUtf8(name ? name : "");
    const QString j = QString::fromUtf8(json ? json : "{}");
    QMetaObject::invokeMethod(
        bridge, [bridge, n, j]() { emit bridge->event(n, j); }, Qt::QueuedConnection);
}

static void on_tray_visible(bool visible) {
    Tray *tray = g_tray.load(std::memory_order_acquire);
    if (!tray) {
        return;
    }
    QMetaObject::invokeMethod(
        tray, [tray, visible]() { tray->setVisible(visible); }, Qt::QueuedConnection);
}

static void on_spinning(bool spinning) {
    Tray *tray = g_tray.load(std::memory_order_acquire);
    if (!tray) {
        return;
    }
    QMetaObject::invokeMethod(
        tray, [tray, spinning]() { tray->setSpinning(spinning); }, Qt::QueuedConnection);
}
}

class NexusApp : public QApplication {
public:
    using QApplication::QApplication;
    void setMainWindow(QWindow *window) { m_mainWindow = window; }

    bool event(QEvent *e) override {
        if (e->type() == QEvent::ApplicationStateChange
            && applicationState() == Qt::ApplicationActive && m_mainWindow
            && !m_mainWindow->isVisible()) {
            m_mainWindow->show();
            m_mainWindow->raise();
            m_mainWindow->requestActivate();
        }
        NexusBridge *bridge = g_bridge.load(std::memory_order_acquire);
        if (e->type() == QEvent::Quit && bridge && !m_allowQuit) {
            const QString raw = bridge->invoke(QStringLiteral("app_quit"), QStringLiteral("{}"));
            const QJsonObject o = QJsonDocument::fromJson(raw.toUtf8()).object();
            if (!o.value(QLatin1String("quit")).toBool()) {
                if (m_mainWindow) {
                    m_mainWindow->show();
                    m_mainWindow->raise();
                    m_mainWindow->requestActivate();
                }
                bridge->requestQuitConfirmation();
                return true;
            }
            m_allowQuit = true;
        }
        return QApplication::event(e);
    }

private:
    QWindow *m_mainWindow = nullptr;
    bool m_allowQuit = false;
};

static void unregisterBackendCallbacks() {
    nexus_set_event_cb(nullptr);
    nexus_set_tray_visible_cb(nullptr);
    nexus_set_spinning_cb(nullptr);
}

int main(int argc, char *argv[]) {
    qputenv("QT_QUICK_CONTROLS_STYLE", "Basic");
    NexusApp app(argc, argv);
    app.setApplicationName(QStringLiteral("Nexus"));
    app.setOrganizationName(QStringLiteral("Nexus"));
    app.setQuitOnLastWindowClosed(false);

    NexusBridge bridge;
    g_bridge.store(&bridge, std::memory_order_release);
    nexus_set_event_cb(on_event);
    nexus_set_tray_visible_cb(on_tray_visible);
    nexus_set_spinning_cb(on_spinning);
    nexus_init();

    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("nexus"), &bridge);

    QObject::connect(&engine, &QQmlApplicationEngine::warnings,
                     [](const QList<QQmlError> &errs) {
                         for (const auto &e : errs) {
                             fprintf(stderr, "qml: %s\n", qPrintable(e.toString()));
                         }
                     });

    const QUrl qml(QStringLiteral("qrc:/nexus/qml/Main.qml"));
    engine.load(qml);
    if (engine.rootObjects().isEmpty()) {
        fprintf(stderr, "qml: failed to load %s\n", qPrintable(qml.toString()));
        unregisterBackendCallbacks();
        nexus_teardown();
        g_bridge.store(nullptr, std::memory_order_release);
        return 1;
    }

    QWindow *win = qobject_cast<QWindow *>(engine.rootObjects().first());
    app.setMainWindow(win);
    Host host(win);
    Tray tray(win, &bridge);
    g_tray.store(&tray, std::memory_order_release);

    const QString snap = bridge.invoke(QStringLiteral("store_snapshot"), QStringLiteral("{}"));
    const QJsonObject st = QJsonDocument::fromJson(snap.toUtf8()).object();
    if (st.value(QLatin1String("hide_tray")).toBool()) {
        tray.setVisible(false);
    }

    const int rc = app.exec();
    unregisterBackendCallbacks();
    nexus_teardown();
    g_tray.store(nullptr, std::memory_order_release);
    g_bridge.store(nullptr, std::memory_order_release);
    return rc;
}
