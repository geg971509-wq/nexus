#include "bridge.h"
#include "nexus_c.h"
#include "qr_decoder.h"

#include <QClipboard>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImageReader>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSet>
#include <QUrl>

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

void NexusBridge::setTrayLabels(QString showWindow, QString quit) {
    showWindow = showWindow.trimmed();
    quit = quit.trimmed();
    if (showWindow.isEmpty())
        showWindow = QStringLiteral("Show Window");
    if (quit.isEmpty())
        quit = QStringLiteral("Quit");
    if (showWindow == m_trayShowWindow && quit == m_trayQuit)
        return;
    m_trayShowWindow = showWindow;
    m_trayQuit = quit;
    emit trayLabelsChanged(m_trayShowWindow, m_trayQuit);
}

QString NexusBridge::trayShowWindowLabel() const {
    return m_trayShowWindow;
}

QString NexusBridge::trayQuitLabel() const {
    return m_trayQuit;
}

void NexusBridge::requestQuitConfirmation() {
    emit quitRequested();
}

QString NexusBridge::decodeQrFile(QString fileUrl) {
    const QUrl url(fileUrl);
    const QString path = url.isLocalFile() ? url.toLocalFile() : fileUrl;
    const QFileInfo info(path);
    auto fail = [](const QString &message) {
        return QString::fromUtf8(
            QJsonDocument(QJsonObject{{QStringLiteral("error"), message}})
                .toJson(QJsonDocument::Compact));
    };
    if (!info.isFile() || !info.isReadable()) {
        return fail(QStringLiteral("image is not readable"));
    }
    constexpr qint64 kMaxImageBytes = 25 * 1024 * 1024;
    if (info.size() > kMaxImageBytes) {
        return fail(QStringLiteral("image exceeds 25 MiB"));
    }

    QImageReader reader(path);
    reader.setAutoTransform(true);
    const QSize dimensions = reader.size();
    constexpr qint64 kMaxPixels = 100LL * 1000 * 1000;
    if (dimensions.isValid()
        && qint64(dimensions.width()) * qint64(dimensions.height()) > kMaxPixels) {
        return fail(QStringLiteral("image dimensions exceed 100 megapixels"));
    }
    const QImage image = reader.read();
    if (image.isNull()) {
        return fail(reader.errorString().isEmpty()
                        ? QStringLiteral("unsupported or damaged image")
                        : reader.errorString());
    }

    const QStringList decoded = QrDecoder().decode(image);
    QJsonArray values;
    QSet<QString> seen;
    for (const QString &payload : decoded) {
        const QString value = payload.trimmed();
        if (!value.isEmpty() && !seen.contains(value)) {
            seen.insert(value);
            values.append(value);
        }
    }
    return QString::fromUtf8(
        QJsonDocument(QJsonObject{{QStringLiteral("values"), values}})
            .toJson(QJsonDocument::Compact));
}
