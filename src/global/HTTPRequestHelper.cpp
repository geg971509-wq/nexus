#include "include/global/HTTPRequestHelper.hpp"

#include <QNetworkProxy>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QNetworkInterface>
#include <QTcpSocket>
#include <QHostAddress>
#include <QTimer>
#include <QFile>
#include <QFileInfo>
#include <QUrl>
#include <QApplication>
#include <QMap>
#include <QStringList>
#include <QProcess>
#include <QProcessEnvironment>
#include <QStandardPaths>



#include "include/global/Configs.hpp"
#include "include/ui/mainwindow.h"
#include "include/global/DeviceDetailsHelper.hpp"

namespace Configs_network {

    namespace {
        // QNetworkAccessManager transfer timeout for plain HTTP requests.
        static constexpr int kHttpTransferTimeoutMs = 10000;
        // How long to wait for the curl helper process to start.
        static constexpr int kCurlStartTimeoutMs = 5000;
    } // namespace

    // Follow proxy settings: Use proxy / system-proxy mode → Throne mixed;
    // otherwise direct OS path (NoProxy). macOS often still stores 127.0.0.1:2080
    // in networksetup with Enabled=No; Qt DefaultProxy can still trip on that.
    // Returns false if proxy was requested but no profile is running.
    static bool applyProxyFromSettings(QNetworkAccessManager &accessManager, bool forceProxy = false) {
        auto *settings = Configs::dataManager->settingsRepo.get();
        const bool useProxy = settings->net_use_proxy || settings->spmode_system_proxy || forceProxy;
        if (!useProxy) {
            accessManager.setProxy(QNetworkProxy(QNetworkProxy::NoProxy));
            return true;
        }
        if (settings->started_id < 0) {
            return false;
        }
        QNetworkProxy p;
        p.setType(QNetworkProxy::HttpProxy);
        p.setHostName(settings->inbound_address == "::" ? "127.0.0.1" : settings->inbound_address);
        p.setPort(settings->inbound_socks_port);
        if (settings->inbound_auth) {
            p.setUser(settings->inbound_user);
            p.setPassword(settings->inbound_pass);
        }
        accessManager.setProxy(p);
        return true;
    }

    static bool wantsThroneProxy(bool forceProxy = false) {
        auto *settings = Configs::dataManager->settingsRepo.get();
        return settings->net_use_proxy || settings->spmode_system_proxy || forceProxy;
    }

    static QString throneMixedProxyUrl() {
        auto *settings = Configs::dataManager->settingsRepo.get();
        const QString host = settings->inbound_address == "::" ? QStringLiteral("127.0.0.1") : settings->inbound_address;
        if (settings->inbound_auth) {
            return QStringLiteral("http://%1:%2@%3:%4")
                .arg(QString::fromUtf8(QUrl::toPercentEncoding(settings->inbound_user)),
                     QString::fromUtf8(QUrl::toPercentEncoding(settings->inbound_pass)),
                     host)
                .arg(settings->inbound_socks_port);
        }
        return QStringLiteral("http://%1:%2").arg(host).arg(settings->inbound_socks_port);
    }

    HTTPResponse NetworkRequestHelper::HttpGet(const QString &url, bool sendHwid, bool useProxy) {
        QNetworkRequest request;
        QNetworkAccessManager accessManager;
        accessManager.setTransferTimeout(kHttpTransferTimeoutMs);
        request.setUrl(url);
        if (!applyProxyFromSettings(accessManager, useProxy)) {
            return HTTPResponse{QObject::tr("Request with proxy but no profile started.")};
        }
        // Set attribute
        request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
        request.setHeader(QNetworkRequest::KnownHeaders::UserAgentHeader, Configs::dataManager->settingsRepo->GetUserAgent());
        if (Configs::dataManager->settingsRepo->net_insecure) {
            QSslConfiguration c;
            c.setPeerVerifyMode(QSslSocket::PeerVerifyMode::VerifyNone);
            request.setSslConfiguration(c);
        }
        //Attach HWID and device info headers if enabled in settings
        if (sendHwid) {
            auto details = GetDeviceDetails();

            // Parse custom parameters if provided
            QMap<QString, QString> customParams;
            if (!Configs::dataManager->settingsRepo->sub_custom_hwid_params.isEmpty()) {
                QStringList pairs = Configs::dataManager->settingsRepo->sub_custom_hwid_params.split(',');
                for (const QString &pair : pairs) {
                    QString trimmed = pair.trimmed();
                    int eqPos = trimmed.indexOf('=');
                    if (eqPos > 0) {
                        QString key = trimmed.left(eqPos).trimmed();
                        QString value = trimmed.mid(eqPos + 1).trimmed();
                        // Validate: key must be one of the allowed parameters, value must not contain newlines
                        if (!key.isEmpty() && !value.isEmpty() &&
                            !value.contains('\n') && !value.contains('\r') &&
                            value.length() < 1000) { // Reasonable length limit
                            QString lowerKey = key.toLower();
                            // Only accept known parameter keys
                            if (lowerKey == "hwid" || lowerKey == "os" ||
                                lowerKey == "osversion" || lowerKey == "model") {
                                customParams[lowerKey] = value;
                            }
                        }
                    }
                }
            }

            // Use custom values if provided, otherwise use default values
            QString hwid = customParams.contains("hwid") ? customParams["hwid"] : details.hwid;
            QString os = customParams.contains("os") ? customParams["os"] : details.os;
            QString osVersion = customParams.contains("osversion") ? customParams["osversion"] : details.osVersion;
            QString model = customParams.contains("model") ? customParams["model"] : details.model;

            if (!hwid.isEmpty()) request.setRawHeader("x-hwid", hwid.toUtf8());
            if (!os.isEmpty()) request.setRawHeader("x-device-os", os.toUtf8());
            if (!osVersion.isEmpty()) request.setRawHeader("x-ver-os", osVersion.toUtf8());
            if (!model.isEmpty()) request.setRawHeader("x-device-model", model.toUtf8());
        }
        //
        auto _reply = accessManager.get(request);
        connect(_reply, &QNetworkReply::sslErrors, _reply, [](const QList<QSslError> &errors) {
            QStringList error_str;
            for (const auto &err: errors) {
                error_str << err.errorString();
            }
            MW_show_log(QString("SSL Errors: %1 %2").arg(error_str.join(","), Configs::dataManager->settingsRepo->net_insecure ? "(Ignored)" : ""));
        });
        // Wait for response
        QEventLoop loop;
        connect(_reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
        loop.exec();

        //
        auto result = HTTPResponse{_reply->error() == QNetworkReply::NetworkError::NoError ? "" : _reply->errorString(),
                                       _reply->readAll(), _reply->rawHeaderPairs()};
        _reply->deleteLater();
        return result;
    }

    QString NetworkRequestHelper::GetHeader(const QList<QPair<QByteArray, QByteArray>> &header, const QString &name) {
        for (const auto &p: header) {
            if (QString(p.first).toLower() == name.toLower()) return p.second;
        }
        return "";
    }

    static QString finalizeDownloadedTmp(const QString &filePath, const QString &tmpPath) {
        QFileInfo info(tmpPath);
        if (!info.exists() || info.size() <= 0) {
            QFile::remove(tmpPath);
            return QObject::tr("Download failed: the server returned an empty response.");
        }
        QFile::remove(filePath);
        if (!QFile::rename(tmpPath, filePath)) {
            QFile::remove(tmpPath);
            return QObject::tr("Could not save downloaded file.");
        }
        return {};
    }

    static bool mixedInboundListening() {
        auto *settings = Configs::dataManager->settingsRepo.get();
        if (settings->started_id < 0) return false;
        const QString host = settings->inbound_address == "::" ? QStringLiteral("127.0.0.1") : settings->inbound_address;
        QTcpSocket sock;
        sock.connectToHost(host, static_cast<quint16>(settings->inbound_socks_port));
        const bool ok = sock.waitForConnected(400);
        if (ok) sock.disconnectFromHost();
        return ok;
    }

    enum class CurlProxyMode {
        Direct,   // follow "Use proxy" off → OS direct
        Mixed,    // Throne mixed inbound
    };

    // One curl attempt. Returns empty on success.
    static QString runCurlDownloadOnce(const QString &url, const QString &fileName,
                                       const QString &tmpPath, CurlProxyMode mode,
                                       const QString &bindHost = {}) {
        QString curlBin = QStringLiteral("/usr/bin/curl");
        if (!QFileInfo::exists(curlBin)) {
            curlBin = QStandardPaths::findExecutable(QStringLiteral("curl"));
        }
        if (curlBin.isEmpty()) {
            return QObject::tr("curl not found");
        }

        auto *settings = Configs::dataManager->settingsRepo.get();
        QStringList args;
        args << QStringLiteral("-4")
             << QStringLiteral("-fsSL")
             << QStringLiteral("--connect-timeout") << QStringLiteral("15")
             << QStringLiteral("--max-time") << QStringLiteral("120")
             << QStringLiteral("-A") << settings->GetUserAgent()
             << QStringLiteral("-o") << tmpPath;
        if (!bindHost.isEmpty()) {
            // Prefer binding by address (more reliable than iface name under split tunnel).
            args << QStringLiteral("--interface") << bindHost;
        }
        if (mode == CurlProxyMode::Mixed) {
            args << QStringLiteral("-x") << throneMixedProxyUrl();
        } else {
            args << QStringLiteral("--noproxy") << QStringLiteral("*");
        }
        args << url;

        QFile::remove(tmpPath);
        QProcess proc;
        QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
        for (const char *k : {"http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY",
                              "ALL_PROXY", "all_proxy", "NO_PROXY", "no_proxy"}) {
            env.remove(QString::fromUtf8(k));
        }
        if (mode == CurlProxyMode::Mixed) {
            const QString proxyUrl = throneMixedProxyUrl();
            env.insert(QStringLiteral("http_proxy"), proxyUrl);
            env.insert(QStringLiteral("https_proxy"), proxyUrl);
            env.insert(QStringLiteral("ALL_PROXY"), proxyUrl);
        } else {
            env.insert(QStringLiteral("NO_PROXY"), QStringLiteral("*"));
            env.insert(QStringLiteral("no_proxy"), QStringLiteral("*"));
        }
        proc.setProcessEnvironment(env);
        proc.setProgram(curlBin);
        proc.setArguments(args);
        proc.start();
        if (!proc.waitForStarted(kCurlStartTimeoutMs)) {
            return QObject::tr("Failed to start curl.");
        }
        while (!proc.waitForFinished(500)) {
            const qint64 n = QFileInfo(tmpPath).size();
            if (n > 0) {
                const QPointer<MainWindow> window(GetMainWindow());
                runOnUiThread([window, fileName, n] {
                    if (!window) return;
                    window->setDownloadReport(DownloadProgressReport{fileName, n, 0}, true);
                    window->UpdateDataView();
                });
            }
        }
        {
            const QPointer<MainWindow> window(GetMainWindow());
            runOnUiThread([window] {
                if (!window) return;
                window->setDownloadReport({}, false);
                window->UpdateDataView(true);
            });
        }
        if (proc.exitStatus() != QProcess::NormalExit || proc.exitCode() != 0) {
            QFile::remove(tmpPath);
            const QString err = QString::fromUtf8(proc.readAllStandardError()).trimmed();
            return err.isEmpty() ? QObject::tr("curl download failed (exit %1).").arg(proc.exitCode()) : err;
        }
        return {};
    }

    static QString firstUtunAddress() {
        const auto ifaces = QNetworkInterface::allInterfaces();
        for (const QNetworkInterface &ifc : ifaces) {
            if (!(ifc.flags() & QNetworkInterface::IsUp)) continue;
            if (!(ifc.flags() & QNetworkInterface::IsRunning)) continue;
            if (!ifc.name().startsWith(QLatin1String("utun"))) continue;
            for (const QNetworkAddressEntry &e : ifc.addressEntries()) {
                const QHostAddress ip = e.ip();
                if (ip.protocol() != QAbstractSocket::IPv4Protocol) continue;
                if (ip.isLoopback()) continue;
                return ip.toString();
            }
        }
        return {};
    }

    static QString explainConnectFailure(const QString &rawErr) {
        QString msg = rawErr;
        msg += QLatin1Char('\n');
        msg += QObject::tr(
            "Throne could not open an outbound HTTPS connection (shell curl may still work). "
            "Common causes on macOS: Little Snitch/LuLu blocking Throne.app, or split-tunnel/WARP path issues.");
        msg += QLatin1Char('\n');
        msg += QObject::tr(
            "Try: allow Throne in Little Snitch; or enable Settings → Use proxy, start a node, then download again. "
            "Local geo files under Preferences/Throne may already be usable.");
        return msg;
    }

    // Primary geo download path: system curl with the same proxy rule as Qt.
    // Returns empty error on success; otherwise error text.
    static QString downloadAssetWithCurl(const QString &url, const QString &fileName,
                                         const QString &filePath, const QString &tmpPath) {
        auto *settings = Configs::dataManager->settingsRepo.get();
        const bool useProxy = wantsThroneProxy();
        if (useProxy && settings->started_id < 0) {
            return QObject::tr("Request with proxy but no profile started.");
        }

        QString lastErr;
        if (useProxy) {
            lastErr = runCurlDownloadOnce(url, fileName, tmpPath, CurlProxyMode::Mixed);
            if (lastErr.isEmpty()) return finalizeDownloadedTmp(filePath, tmpPath);
            return lastErr;
        }

        // Use proxy OFF → direct first (product rule).
        lastErr = runCurlDownloadOnce(url, fileName, tmpPath, CurlProxyMode::Direct);
        if (lastErr.isEmpty()) return finalizeDownloadedTmp(filePath, tmpPath);

        // Under WARP split tunnel, binding to utun address often matches shell routing.
        const QString utunIp = firstUtunAddress();
        if (!utunIp.isEmpty()) {
            MW_show_log(QString("DownloadAsset direct curl failed (%1); retry bind %2 for %3")
                            .arg(lastErr, utunIp, fileName));
            const QString bindErr = runCurlDownloadOnce(url, fileName, tmpPath, CurlProxyMode::Direct, utunIp);
            if (bindErr.isEmpty()) return finalizeDownloadedTmp(filePath, tmpPath);
            lastErr = bindErr;
        }

        // Optional recovery only when a profile mixed port is actually up — still not
        // "always force proxy"; only a second path after direct failed.
        if (mixedInboundListening()) {
            MW_show_log(QString("DownloadAsset direct curl failed (%1); retry via mixed for %2")
                            .arg(lastErr, fileName));
            const QString mixedErr = runCurlDownloadOnce(url, fileName, tmpPath, CurlProxyMode::Mixed);
            if (mixedErr.isEmpty()) return finalizeDownloadedTmp(filePath, tmpPath);
            lastErr = mixedErr;
        }

        if (lastErr.contains(QLatin1String("Failed to connect"), Qt::CaseInsensitive) ||
            lastErr.contains(QLatin1String("Couldn't connect"), Qt::CaseInsensitive) ||
            lastErr.contains(QLatin1String("Connection refused"), Qt::CaseInsensitive)) {
            return explainConnectFailure(lastErr);
        }
        return lastErr;
    }

    static QString downloadAssetWithQt(const QString &url, const QString &fileName,
                                       const QString &filePath, const QString &tmpPath) {
        QFile::remove(tmpPath);

        QNetworkRequest request;
        QNetworkAccessManager accessManager;
        accessManager.setTransferTimeout(120000);
        request.setUrl(url);
        if (!applyProxyFromSettings(accessManager)) {
            return QObject::tr("Request with proxy but no profile started.");
        }
        request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
        request.setHeader(QNetworkRequest::KnownHeaders::UserAgentHeader, Configs::dataManager->settingsRepo->GetUserAgent());
        if (Configs::dataManager->settingsRepo->net_insecure) {
            QSslConfiguration c;
            c.setPeerVerifyMode(QSslSocket::PeerVerifyMode::VerifyNone);
            request.setSslConfiguration(c);
        }

        QFile tmp(tmpPath);
        if (!tmp.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
            return QObject::tr("Could not open file.");
        }

        auto *_reply = accessManager.get(request);
        QObject::connect(_reply, &QNetworkReply::sslErrors, _reply, [](const QList<QSslError> &errors) {
            QStringList error_str;
            for (const auto &err: errors) {
                error_str << err.errorString();
            }
            MW_show_log(QString("SSL Errors: %1 %2").arg(error_str.join(","), Configs::dataManager->settingsRepo->net_insecure ? "(Ignored)" : ""));
        });
        QObject::connect(_reply, &QNetworkReply::readyRead, _reply, [&] {
            const QByteArray chunk = _reply->readAll();
            if (!chunk.isEmpty() && tmp.write(chunk) != chunk.size()) {
                _reply->abort();
            }
        });
        QObject::connect(_reply, &QNetworkReply::downloadProgress, _reply, [&](qint64 bytesReceived, qint64 bytesTotal) {
            const QPointer<MainWindow> window(GetMainWindow());
            runOnUiThread([window, fileName, bytesReceived, bytesTotal] {
                if (!window) return;
                window->setDownloadReport(DownloadProgressReport{fileName, bytesReceived, bytesTotal}, true);
                window->UpdateDataView();
            });
        });
        QEventLoop loop;
        QObject::connect(_reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
        loop.exec();

        if (_reply->bytesAvailable() > 0) {
            const QByteArray chunk = _reply->readAll();
            if (!chunk.isEmpty()) tmp.write(chunk);
        }
        tmp.flush();
        tmp.close();

        {
            const QPointer<MainWindow> window(GetMainWindow());
            runOnUiThread([window] {
                if (!window) return;
                window->setDownloadReport({}, false);
                window->UpdateDataView(true);
            });
        }

        const auto netErr = _reply->error();
        const QString netErrStr = _reply->errorString();
        const int httpStatus = _reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        _reply->deleteLater();

        if (netErr == QNetworkReply::NetworkError::NoError &&
            (httpStatus == 0 || (httpStatus >= 200 && httpStatus < 300))) {
            return finalizeDownloadedTmp(filePath, tmpPath);
        }
        QFile::remove(tmpPath);
        if (!netErrStr.isEmpty()) return netErrStr;
        if (httpStatus != 0) {
            return QObject::tr("Download failed: server returned HTTP status %1.").arg(httpStatus);
        }
        return QObject::tr("Download failed.");
    }

    QString NetworkRequestHelper::DownloadAsset(const QString &url, const QString &fileName) {
        const auto filePath = Configs::GetBasePath() + "/" + fileName;
        const auto tmpPath = filePath + ".tmp";

        // curl first: Qt QNAM on worker threads often dies under Tun/WARP
        // ("Invalid socket descriptor" / "Socket is not connected").
        const QString curlErr = downloadAssetWithCurl(url, fileName, filePath, tmpPath);
        if (curlErr.isEmpty()) return {};

        MW_show_log(QString("DownloadAsset curl failed (%1); falling back to Qt for %2")
                        .arg(curlErr, fileName));
        const QString qtErr = downloadAssetWithQt(url, fileName, filePath, tmpPath);
        if (qtErr.isEmpty()) return {};
        return QObject::tr("%1 (Qt fallback: %2)").arg(curlErr, qtErr);
    }

} // namespace Configs_network
