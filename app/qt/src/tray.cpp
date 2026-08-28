#include "tray.h"
#include "bridge.h"

#include <QAction>
#include <QCoreApplication>
#include <QMenu>
#include <QSystemTrayIcon>
#include <QTimer>
#include <QWindow>

Tray::Tray(QWindow *window, NexusBridge *bridge, QObject *parent)
    : QObject(parent)
    , m_window(window)
    , m_bridge(bridge) {
    loadFrames();
    m_icon = new QSystemTrayIcon(this);
    m_icon->setToolTip(QStringLiteral("Nexus"));
    applyFrame(0);

    m_menu = new QMenu();
    m_showAction = m_menu->addAction(QStringLiteral("Show Window"));
    m_quitAction = m_menu->addAction(QStringLiteral("Quit"));
    connect(m_showAction, &QAction::triggered, this, &Tray::showWindow);
    connect(m_quitAction, &QAction::triggered, this, &Tray::quitApp);
    if (m_bridge) {
        connect(m_bridge, &NexusBridge::trayLabelsChanged,
                this, &Tray::setLabels);
        setLabels(m_bridge->trayShowWindowLabel(), m_bridge->trayQuitLabel());
    }
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

Tray::~Tray() {
    if (m_icon)
        m_icon->setContextMenu(nullptr);
    delete m_menu;
}

void Tray::loadFrames() {
    m_frames.clear();
    for (int i = 0; i < 12; ++i) {
        const QString path = QStringLiteral(":/nexus/tray/frame_%1.png")
                                 .arg(i, 2, 10, QChar('0'));
        const QIcon frame(path);
        if (frame.isNull()) {
            continue;
        }
        m_frames.push_back(frame);
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

void Tray::setLabels(const QString &showWindow, const QString &quit) {
    if (m_showAction)
        m_showAction->setText(showWindow);
    if (m_quitAction)
        m_quitAction->setText(quit);
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
    showWindow();
    if (m_bridge) {
        m_bridge->requestQuitConfirmation();
        return;
    }
    QCoreApplication::quit();
}
