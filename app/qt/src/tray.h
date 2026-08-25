#pragma once

#include <QIcon>
#include <QObject>
#include <QVector>

class QAction;
class QMenu;
class QSystemTrayIcon;
class QTimer;
class QWindow;
class NexusBridge;

class Tray : public QObject {
    Q_OBJECT
public:
    explicit Tray(QWindow *window, NexusBridge *bridge, QObject *parent = nullptr);
    void setVisible(bool visible);
    void setSpinning(bool on);
    void showWindow();

private:
    void loadFrames();
    void applyFrame(int index);
    void quitApp();

    QWindow *m_window;
    NexusBridge *m_bridge;
    QSystemTrayIcon *m_icon = nullptr;
    QMenu *m_menu = nullptr;
    QTimer *m_timer = nullptr;
    QVector<QIcon> m_frames;
    int m_index = 0;
    bool m_spinning = false;
};
