#pragma once
#include "include/configs/common/multiplex.h"
#include "include/configs/common/Outbound.h"
#include "include/configs/common/TLS.h"
#include "include/configs/common/transport.h"

namespace Configs
{

    inline QStringList vmessSecurity = {"auto", "none", "zero", "aes-128-gcm", "chacha20-poly1305"};

    class vmess : public outbound
    {
        public:
        QString uuid;
        QString security = "auto";
        int alter_id = 0;
        bool global_padding = false;
        bool authenticated_length = false;
        std::shared_ptr<TLS> tls = std::make_shared<TLS>();
        QString packet_encoding = "xudp";
        std::shared_ptr<Transport> transport = std::make_shared<Transport>();
        std::shared_ptr<Multiplex> multiplex = std::make_shared<Multiplex>();

        bool HasTLS() const override {
            return true;
        }

        bool HasMux() const override {
            return true;
        }

        bool HasTransport() const override {
            return true;
        }

        std::shared_ptr<TLS> GetTLS() const override {
            return tls;
        }

        std::shared_ptr<Multiplex> GetMux() const override {
            return multiplex;
        }

        std::shared_ptr<Transport> GetTransport() const override {
            return transport;
        }

        // baseConfig overrides
        bool ParseFromLink(const QString& link) override;
        bool ParseFromJson(const QJsonObject& object) override;
        bool ParseFromClash(const clash::Proxies& object) override;
        QString ExportToLink() const override;
        QJsonObject ExportToJson() const override;
        BuildResult Build() const override;

        QString DisplayType() const override;
        SecurityInfo GetSecurity() const override;
    };
}
