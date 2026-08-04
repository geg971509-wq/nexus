#pragma once

#include <QMessageBox>
#include <QTimer>

class MessageBoxTimer : public QTimer {
public:
    QMessageBox *msgbox = nullptr;
    bool showed = false;

    explicit MessageBoxTimer(QObject *parent, QMessageBox *msgbox, int delayMs) : QTimer(parent) {
        connect(this, &QTimer::timeout, this, &MessageBoxTimer::timeoutFunc, Qt::ConnectionType::QueuedConnection);
        this->msgbox = msgbox;
        setSingleShot(true);
        setInterval(delayMs);
        start();
    };

    void cancel() {
        QTimer::stop();
        if (msgbox != nullptr && showed) {
            msgbox->reject(); // closes the non-modal advisory
        }
    };

private:
    // Deliberately not exec(): this box is advisory ("things are slow, consider
    // restarting") and appears while a start/stop is still in flight. exec()
    // would make it application-modal and swallow every click on the main
    // window -- including the start/stop button's own Cancel -- so the user's
    // only way out of a slow start would be the very restart it suggests.
    void timeoutFunc() {
        if (msgbox == nullptr) return;
        showed = true;
        msgbox->setModal(false);
        msgbox->setWindowModality(Qt::NonModal);
        msgbox->show();
    }
};
