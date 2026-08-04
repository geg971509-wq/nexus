#pragma once
#include "include/configs/common/multiplex.h"
#include "include/configs/common/Outbound.h"
#include "include/configs/common/TLS.h"
#include "include/configs/common/transport.h"

namespace Configs
{
    inline QStringList vlessFlows = {"xtls-rprx-vision"};

    class vless : public outbound
    {
        public:
        QString uuid;
        QString flow;
        std::shared_ptr<TLS> tls = std::make_shared<TLS>();
        QString packet_encoding = "xudp";
        std::shared_ptr<Multiplex> multiplex = std::make_shared<Multiplex>();
        std::shared_ptr<Transport> transport = std::make_shared<Transport>();

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
    };
}
