#pragma once

#include <QObject>
#include <QString>

class NexusBridge : public QObject {
    Q_OBJECT
public:
    explicit NexusBridge(QObject *parent = nullptr);
    Q_INVOKABLE QString invoke(QString cmd, QString json);
    Q_INVOKABLE QString clipboardText();
    Q_INVOKABLE void setClipboardText(QString text);

signals:
    void event(QString name, QString json);
};
