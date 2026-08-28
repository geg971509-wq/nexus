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
    Q_INVOKABLE QString decodeQrFile(QString fileUrl);
    Q_INVOKABLE void setTrayLabels(QString showWindow, QString quit);
    QString trayShowWindowLabel() const;
    QString trayQuitLabel() const;
    void requestQuitConfirmation();

signals:
    void event(QString name, QString json);
    void quitRequested();
    void trayLabelsChanged(QString showWindow, QString quit);

private:
    QString m_trayShowWindow = QStringLiteral("Show Window");
    QString m_trayQuit = QStringLiteral("Quit");
};
