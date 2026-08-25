#include "bridge.h"
#include "nexus_c.h"

#include <QGuiApplication>
#include <QClipboard>

NexusBridge::NexusBridge(QObject *parent) : QObject(parent) {}

QString NexusBridge::invoke(QString cmd, QString json) {
    const QByteArray cmdUtf = cmd.toUtf8();
    const QByteArray jsonUtf = json.toUtf8();
    char *raw = nexus_invoke(cmdUtf.constData(), jsonUtf.constData());
    const QString out = raw ? QString::fromUtf8(raw) : QStringLiteral("{}");
    nexus_free(raw);
    return out;
}

QString NexusBridge::clipboardText() {
    QClipboard *clip = QGuiApplication::clipboard();
    return clip ? clip->text() : QString();
}

void NexusBridge::setClipboardText(QString text) {
    QClipboard *clip = QGuiApplication::clipboard();
    if (clip)
        clip->setText(text);
}

