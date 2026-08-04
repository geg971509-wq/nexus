#pragma once

#include <QJsonObject>
#include <QList>
#include <QString>

#include <memory>

namespace Configs_network {
    inline const QString warpApiURL = "https://api.cloudflareclient.com/v0a1922/reg";

    struct warpAccount {
        QString deviceId;
        QString accessToken;
        QString license;
        QString localPublicKey;
        QJsonObject accountState;

        [[nodiscard]] bool hasCredentials() const;
        [[nodiscard]] QJsonObject toJson() const;
        bool fromJson(const QJsonObject& object);
    };

    struct warpConfig {
        QString privateKey;
        QString publicKey;
        QString endpoint;
        QString endpointAddress;
        int endpointPort = 0;
        QString ipv4Address;
        QString ipv6Address;
        QList<int> reserved;
        warpAccount account;
    };

    std::shared_ptr<warpConfig> registerWarpConfig(QString* error, const QString& privateKey,
                                                   const QString& publicKey);
    std::shared_ptr<warpConfig> refreshWarpConfig(QString* error, const warpAccount& account);
}
