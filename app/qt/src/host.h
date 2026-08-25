#pragma once

#include <QObject>

class QWindow;

class Host : public QObject {
    Q_OBJECT
public:
    explicit Host(QWindow *window, QObject *parent = nullptr);
    void showWindow();

protected:
    bool eventFilter(QObject *obj, QEvent *event) override;

private:
    QWindow *m_window;
};
