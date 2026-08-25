#include "tray.h"
#include "bridge.h"

#include <QAction>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QMenu>
#include <QSystemTrayIcon>
#include <QTimer>
#include <QWindow>

#ifndef NEXUS_TRAY_DIR
#define NEXUS_TRAY_DIR ""
#endif

Tray::Tray(QWindow *window, NexusBridge *bridge, QObject *parent)
    : QObject(parent)
    , m_window(window)
    , m_bridge(bridge) {
    Q_UNUSED(m_bridge);
    loadFrames();
    m_icon = new QSystemTrayIcon(this);
    m_icon->setToolTip(QStringLiteral("Nexus"));
    applyFrame(0);

    m_menu = new QMenu();
    auto *showAct = m_menu->addAction(QStringLiteral("Show Window"));
    auto *quitAct = m_menu->addAction(QStringLiteral("Quit"));
    connect(showAct, &QAction::triggered, this, &Tray::showWindow);
    connect(quitAct, &QAction::triggered, this, &Tray::quitApp);
    m_icon->setContextMenu(m_menu);
    m_icon->setVisible(true);

    connect(m_icon, &QSystemTrayIcon::activated, this,
            [this](QSystemTrayIcon::ActivationReason reason) {
                if (reason == QSystemTrayIcon::Trigger) {
                    showWindow();
                }
            });

    m_timer = new QTimer(this);
    m_timer->setInterval(90);
    connect(m_timer, &QTimer::timeout, this, [this]() {
        if (!m_spinning || m_frames.isEmpty()) {
            return;
        }
        m_index = (m_index + 1) % m_frames.size();
        applyFrame(m_index);
    });
}

void Tray::loadFrames() {
    const QDir dir(QString::fromUtf8(NEXUS_TRAY_DIR));
    m_frames.clear();
    for (int i = 0; i < 12; ++i) {
        const QString name = QStringLiteral("frame_%1.png").arg(i, 2, 10, QChar('0'));
        const QString path = dir.filePath(name);
        if (!QFile::exists(path)) {
            continue;
        }
        m_frames.push_back(QIcon(path));
    }
}

void Tray::applyFrame(int index) {
    if (!m_icon) {
        return;
    }
    if (m_frames.isEmpty()) {
        return;
    }
    const int i = index % m_frames.size();
    m_icon->setIcon(m_frames.at(i));
}

void Tray::setVisible(bool visible) {
    if (m_icon) {
        m_icon->setVisible(visible);
    }
}

void Tray::setSpinning(bool on) {
    m_spinning = on;
    if (on) {
        m_index = 0;
        applyFrame(0);
        m_timer->start();
    } else {
        m_timer->stop();
        m_index = 0;
        applyFrame(0);
    }
}

void Tray::showWindow() {
    if (!m_window) {
        return;
    }
    m_window->show();
    m_window->raise();
    m_window->requestActivate();
}

void Tray::quitApp() {
    QCoreApplication::quit();
}
