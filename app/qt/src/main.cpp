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
#include <cstdio>

#ifndef NEXUS_QML_DIR
#define NEXUS_QML_DIR ""
#endif

static Tray *g_tray = nullptr;
static NexusBridge *g_bridge = nullptr;

extern "C" {
static void on_event(const char *name, const char *json) {
    if (!g_bridge) {
        return;
    }
    const QString n = QString::fromUtf8(name ? name : "");
    const QString j = QString::fromUtf8(json ? json : "{}");
    QMetaObject::invokeMethod(
        g_bridge, [n, j]() { emit g_bridge->event(n, j); }, Qt::QueuedConnection);
}

static void on_tray_visible(bool visible) {
    if (!g_tray) {
        return;
    }
    QMetaObject::invokeMethod(
        g_tray, [visible]() { g_tray->setVisible(visible); }, Qt::QueuedConnection);
}

static void on_spinning(bool spinning) {
    if (!g_tray) {
        return;
    }
    QMetaObject::invokeMethod(
        g_tray, [spinning]() { g_tray->setSpinning(spinning); }, Qt::QueuedConnection);
}
}

class NexusApp : public QApplication {
public:
    using QApplication::QApplication;
    bool event(QEvent *e) override {
        if (e->type() == QEvent::Quit && g_bridge && !m_allowQuit) {
            const QString raw = g_bridge->invoke(QStringLiteral("app_quit"), QStringLiteral("{}"));
            const QJsonObject o = QJsonDocument::fromJson(raw.toUtf8()).object();
            if (!o.value(QLatin1String("quit")).toBool()) {
                return true;
            }
            m_allowQuit = true;
        }
        return QApplication::event(e);
    }

private:
    bool m_allowQuit = false;
};

int main(int argc, char *argv[]) {
    qputenv("QT_QUICK_CONTROLS_STYLE", "Basic");
    NexusApp app(argc, argv);
    app.setApplicationName(QStringLiteral("Nexus"));
    app.setOrganizationName(QStringLiteral("Nexus"));
    app.setQuitOnLastWindowClosed(false);

    nexus_set_event_cb(on_event);
    nexus_set_tray_visible_cb(on_tray_visible);
    nexus_set_spinning_cb(on_spinning);
    nexus_init();

    NexusBridge bridge;
    g_bridge = &bridge;
    QQmlApplicationEngine engine;
    engine.addImportPath(QString::fromUtf8(NEXUS_QML_DIR));
    engine.rootContext()->setContextProperty(QStringLiteral("nexus"), &bridge);

    QObject::connect(&engine, &QQmlApplicationEngine::warnings,
                     [](const QList<QQmlError> &errs) {
                         for (const auto &e : errs) {
                             fprintf(stderr, "qml: %s\n", qPrintable(e.toString()));
                         }
                     });

    const QString qml = QString::fromUtf8(NEXUS_QML_DIR) + QStringLiteral("/Main.qml");
    engine.load(QUrl::fromLocalFile(qml));
    if (engine.rootObjects().isEmpty()) {
        fprintf(stderr, "qml: failed to load %s\n", qPrintable(qml));
        g_bridge = nullptr;
        return 1;
    }

    QWindow *win = qobject_cast<QWindow *>(engine.rootObjects().first());
    Host host(win);
    Tray tray(win, &bridge);
    g_tray = &tray;

    const QString snap = bridge.invoke(QStringLiteral("store_snapshot"), QStringLiteral("{}"));
    const QJsonObject st = QJsonDocument::fromJson(snap.toUtf8()).object();
    if (st.value(QLatin1String("hide_tray")).toBool()) {
        tray.setVisible(false);
    }

    const int rc = app.exec();
    g_tray = nullptr;
    g_bridge = nullptr;
    nexus_teardown();
    return rc;
}
