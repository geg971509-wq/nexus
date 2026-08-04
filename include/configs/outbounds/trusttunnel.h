#pragma once
#include "include/configs/common/Outbound.h"
#include "include/configs/common/TLS.h"

namespace Configs
{
    class trusttunnel : public outbound
    {
        public:
        QString username;
        QString password;
        QString congestion_control;
        bool health_check = false;
        bool quic = false;
        std::shared_ptr<TLS> tls = std::make_shared<TLS>();

        bool HasTLS() const override {
            return true;
        }

        bool MustTLS() const override {
            return true;
        }

        std::shared_ptr<TLS> GetTLS() const override {
            return tls;
        }

        // baseConfig overrides
        bool ParseFromLink(const QString& link) override;
        bool ParseFromJson(const QJsonObject& object) override;
        QString ExportToLink() const override;
        QJsonObject ExportToJson() const override;
        BuildResult Build() const override;

        QString DisplayType() const override;
        SecurityInfo GetSecurity() const override;
    };
}
