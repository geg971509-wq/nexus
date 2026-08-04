#include "include/configs/sub/warp.h"

#include <QDateTime>
#include <QEventLoop>
#include <QHostAddress>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonParseError>
#include <QNetworkAccessManager>
#include <QNetworkProxy>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QSslConfiguration>
#include <QSslSocket>
#include <QUrl>

#include "include/global/Configs.hpp"
#include "include/global/Utils.hpp"

namespace Configs_network {
    namespace {
        constexpr qsizetype maxResponseBytes = 1 << 20;

        struct HttpResponse {
            QByteArray body;
            QString error;
        };

        bool validOpaqueString(const QString& value)
        {
            return !value.isEmpty() && value.size() <= 4096 && value == value.trimmed()
                   && !value.contains('\r') && !value.contains('\n');
        }

        bool validWireguardKey(const QString& value)
        {
            return DecodeB64IfValid(value).size() == 32;
        }

        bool requiredString(const QJsonObject& object, const QString& key, QString* value,
                            QString* error)
        {
            const auto item = object.value(key);
            if (!item.isString() || !validOpaqueString(item.toString())) {
                *error = QObject::tr("Invalid WARP response: missing or invalid %1").arg(key);
                return false;
            }
            *value = item.toString();
            return true;
        }

        bool parseEndpoint(const QString& endpoint, QString* address, int* port)
        {
            const QUrl url("udp://" + endpoint, QUrl::StrictMode);
            if (!url.isValid() || url.host().isEmpty() || !IsValidPort(url.port(-1))
                || !url.userInfo().isEmpty() || !url.path().isEmpty() || url.hasQuery()
                || url.hasFragment()) {
                return false;
            }
            *address = url.host();
            *port = url.port();
            return true;
        }

        HttpResponse sendRequest(QNetworkAccessManager::Operation operation, const QUrl& url,
                                 const QByteArray& payload, const QString& token)
        {
            QNetworkAccessManager manager;
            manager.setTransferTimeout(20000);

            if (Configs::dataManager->settingsRepo->net_use_proxy
                || Configs::dataManager->settingsRepo->spmode_system_proxy) {
                if (Configs::dataManager->settingsRepo->started_id < 0) {
                    return {{}, QObject::tr("Request with proxy but no profile started.")};
                }
                QNetworkProxy proxy;
                proxy.setType(QNetworkProxy::HttpProxy);
                proxy.setHostName(Configs::dataManager->settingsRepo->inbound_address == "::"
                                      ? "127.0.0.1"
                                      : Configs::dataManager->settingsRepo->inbound_address);
                proxy.setPort(Configs::dataManager->settingsRepo->inbound_socks_port);
                if (Configs::dataManager->settingsRepo->inbound_auth) {
                    proxy.setUser(Configs::dataManager->settingsRepo->inbound_user);
                    proxy.setPassword(Configs::dataManager->settingsRepo->inbound_pass);
                }
                manager.setProxy(proxy);
            }

            QNetworkRequest request(url);
            request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                                 QNetworkRequest::NoLessSafeRedirectPolicy);
            request.setAttribute(QNetworkRequest::Http2AllowedAttribute, false);
            request.setHeader(QNetworkRequest::ContentTypeHeader, "application/json");
            request.setHeader(QNetworkRequest::UserAgentHeader, "okhttp/3.12.1");
            request.setRawHeader("CF-Client-Version", "a-6.3-1922");
            if (!token.isEmpty()) {
                request.setRawHeader("Authorization", "Bearer " + token.toUtf8());
            }
            auto ssl = QSslConfiguration::defaultConfiguration();
            ssl.setProtocol(QSsl::TlsV1_2);
            request.setSslConfiguration(ssl);

            QNetworkReply* reply = operation == QNetworkAccessManager::PostOperation
                                       ? manager.post(request, payload)
                                       : manager.get(request);
            reply->setReadBufferSize(maxResponseBytes + 1);

            QByteArray body;
            bool oversized = false;
            QEventLoop loop;
            const auto drain = [&] {
                const auto chunk = reply->readAll();
                if (chunk.size() > maxResponseBytes - body.size()) {
                    oversized = true;
                    reply->abort();
                    return;
                }
                body += chunk;
            };
            QObject::connect(reply, &QNetworkReply::readyRead, &loop, drain);
            QObject::connect(reply, &QNetworkReply::metaDataChanged, &loop, [&] {
                bool ok = false;
                const auto contentLength = reply->header(QNetworkRequest::ContentLengthHeader).toLongLong(&ok);
                if (ok && contentLength > maxResponseBytes) {
                    oversized = true;
                    reply->abort();
                }
            });
            QObject::connect(reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);
            loop.exec();
            if (!oversized) drain();

            if (oversized) return {{}, QObject::tr("WARP response exceeds 1 MiB")};

            const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            if (status < 200 || status >= 300) {
                return {{}, QObject::tr("WARP request failed: HTTP %1").arg(status)};
            }
            if (reply->error() != QNetworkReply::NoError) return {{}, reply->errorString()};
            return {body, {}};
        }

        std::shared_ptr<warpConfig> parseResponse(const QByteArray& body,
                                                  const QString& localPrivateKey,
                                                  const QString& localPublicKey,
                                                  const QString& expectedDeviceId,
                                                  QString* error)
        {
            QJsonParseError parseError;
            const auto document = QJsonDocument::fromJson(body, &parseError);
            if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
                *error = QObject::tr("Invalid WARP response JSON");
                return {};
            }

            const auto root = document.object();
            auto config = std::make_shared<warpConfig>();
            config->privateKey = localPrivateKey;
            config->account.localPublicKey = localPublicKey;

            if (!requiredString(root, "id", &config->account.deviceId, error)
                || !requiredString(root, "token", &config->account.accessToken, error)) {
                return {};
            }
            if (!expectedDeviceId.isEmpty() && config->account.deviceId != expectedDeviceId) {
                *error = QObject::tr("Invalid WARP response: device id mismatch");
                return {};
            }

            const auto accountValue = root.value("account");
            if (!accountValue.isObject() || accountValue.toObject().isEmpty()) {
                *error = QObject::tr("Invalid WARP response: missing account state");
                return {};
            }
            config->account.accountState = accountValue.toObject();
            if (!requiredString(config->account.accountState, "license", &config->account.license,
                                error)) {
                return {};
            }

            const auto configValue = root.value("config");
            if (!configValue.isObject()) {
                *error = QObject::tr("Invalid WARP response: missing config");
                return {};
            }
            const auto deviceConfig = configValue.toObject();
            const auto peersValue = deviceConfig.value("peers");
            if (!peersValue.isArray() || peersValue.toArray().isEmpty()
                || !peersValue.toArray().first().isObject()) {
                *error = QObject::tr("Invalid WARP response: missing peer");
                return {};
            }
            const auto peer = peersValue.toArray().first().toObject();
            if (!requiredString(peer, "public_key", &config->publicKey, error)
                || !validWireguardKey(config->publicKey)) {
                *error = QObject::tr("Invalid WARP response: invalid peer public key");
                return {};
            }

            const auto endpointValue = peer.value("endpoint");
            if (!endpointValue.isObject()
                || !requiredString(endpointValue.toObject(), "host", &config->endpoint, error)
                || !parseEndpoint(config->endpoint, &config->endpointAddress,
                                  &config->endpointPort)) {
                *error = QObject::tr("Invalid WARP response: invalid endpoint");
                return {};
            }

            const auto interfaceValue = deviceConfig.value("interface");
            if (!interfaceValue.isObject()
                || !interfaceValue.toObject().value("addresses").isObject()) {
                *error = QObject::tr("Invalid WARP response: missing interface addresses");
                return {};
            }
            const auto addresses = interfaceValue.toObject().value("addresses").toObject();
            if (!requiredString(addresses, "v4", &config->ipv4Address, error)
                || !requiredString(addresses, "v6", &config->ipv6Address, error)
                || QHostAddress(config->ipv4Address).protocol() != QAbstractSocket::IPv4Protocol
                || QHostAddress(config->ipv6Address).protocol() != QAbstractSocket::IPv6Protocol) {
                *error = QObject::tr("Invalid WARP response: invalid interface addresses");
                return {};
            }

            QString clientId;
            if (!requiredString(deviceConfig, "client_id", &clientId, error)) return {};
            const auto reserved = DecodeB64IfValid(clientId);
            if (reserved.size() != 3) {
                *error = QObject::tr("Invalid WARP response: invalid reserved bytes");
                return {};
            }
            for (const auto byte : reserved) {
                config->reserved.append(static_cast<unsigned char>(byte));
            }
            return config;
        }
    }

    bool warpAccount::hasCredentials() const
    {
        return validOpaqueString(deviceId) && validOpaqueString(accessToken)
               && validWireguardKey(localPublicKey) && validOpaqueString(license)
               && !accountState.isEmpty();
    }

    QJsonObject warpAccount::toJson() const
    {
        if (!hasCredentials()) return {};
        return {
            {"device_id", deviceId},
            {"access_token", accessToken},
            {"license", license},
            {"local_public_key", localPublicKey},
            {"account_state", accountState},
        };
    }

    bool warpAccount::fromJson(const QJsonObject& object)
    {
        warpAccount parsed;
        const auto state = object.value("account_state");
        if (!object.value("device_id").isString() || !object.value("access_token").isString()
            || !object.value("license").isString()
            || !object.value("local_public_key").isString() || !state.isObject()) {
            return false;
        }
        parsed.deviceId = object.value("device_id").toString();
        parsed.accessToken = object.value("access_token").toString();
        parsed.license = object.value("license").toString();
        parsed.localPublicKey = object.value("local_public_key").toString();
        parsed.accountState = state.toObject();
        if (!parsed.hasCredentials()
            || parsed.accountState.value("license").toString() != parsed.license) {
            return false;
        }
        *this = std::move(parsed);
        return true;
    }

    std::shared_ptr<warpConfig> registerWarpConfig(QString* error, const QString& privateKey,
                                                   const QString& publicKey)
    {
        error->clear();
        if (!validWireguardKey(privateKey) || !validWireguardKey(publicKey)) {
            *error = QObject::tr("Invalid local WireGuard keypair");
            return {};
        }

        const QJsonObject payload = {
            {"fcm_token", ""},
            {"install_id", ""},
            {"key", publicKey},
            {"locale", "en_US"},
            {"model", "PC"},
            {"tos", QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs)},
            {"type", "Android"},
            {"warp_enabled", true},
        };
        const auto response = sendRequest(QNetworkAccessManager::PostOperation, QUrl(warpApiURL),
                                          QJsonDocument(payload).toJson(QJsonDocument::Compact), {});
        if (!response.error.isEmpty()) {
            *error = response.error;
            return {};
        }
        return parseResponse(response.body, privateKey, publicKey, {}, error);
    }

    std::shared_ptr<warpConfig> refreshWarpConfig(QString* error, const warpAccount& account)
    {
        error->clear();
        if (!account.hasCredentials()) {
            *error = QObject::tr("Cached WARP account credentials are incomplete");
            return {};
        }
        const auto devicePath = QString::fromUtf8(QUrl::toPercentEncoding(account.deviceId));
        const auto response = sendRequest(QNetworkAccessManager::GetOperation,
                                          QUrl(warpApiURL + "/" + devicePath), {},
                                          account.accessToken);
        if (!response.error.isEmpty()) {
            *error = response.error;
            return {};
        }
        auto config = parseResponse(response.body, {}, account.localPublicKey, account.deviceId,
                                    error);
        if (!config) return {};
        if (config->account.license != account.license) {
            *error = QObject::tr("Invalid WARP response: license mismatch");
            return {};
        }
        const auto accountState = config->account.accountState;
        config->account = account;
        config->account.accountState = accountState;
        return config;
    }
}
