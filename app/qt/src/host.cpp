#include "host.h"
#ifdef Q_OS_MACOS
#include "macos.h"
#endif

#include <QEvent>
#include <QGuiApplication>
#include <QWindow>

Host::Host(QWindow *window, QObject *parent)
    : QObject(parent)
    , m_window(window) {
    if (!m_window) {
        return;
    }
    m_window->resize(1000, 668);
    m_window->setMinimumSize(QSize(900, 550));
    m_window->installEventFilter(this);
#ifdef Q_OS_MACOS
    styleMacosMainWindow(m_window);
#endif
    connect(qApp, &QGuiApplication::applicationStateChanged, this,
            [this](Qt::ApplicationState state) {
                if (state == Qt::ApplicationActive && m_window && !m_window->isVisible()) {
                    showWindow();
                }
            });
}

void Host::showWindow() {
    if (!m_window) {
        return;
    }
    m_window->show();
    m_window->raise();
    m_window->requestActivate();
}

bool Host::eventFilter(QObject *obj, QEvent *event) {
    if (obj != m_window) {
        return QObject::eventFilter(obj, event);
    }
#ifdef Q_OS_MACOS
    if (event->type() == QEvent::Show || event->type() == QEvent::WinIdChange ||
        event->type() == QEvent::Resize) {
        styleMacosMainWindow(m_window);
    }
#endif
    if (event->type() == QEvent::Close) {
        event->ignore();
        m_window->hide();
        return true;
    }
    return QObject::eventFilter(obj, event);
}
