#pragma once
#include "include/configs/common/Outbound.h"
#include "include/configs/common/xrayMultiplex.h"
#include "include/configs/common/xrayStreamSetting.h"

namespace Configs {
    inline QStringList xrayFlows = {"xtls-rprx-vision", "xtls-rprx-vision-udp443"};

    class xrayVless : public outbound {
        public:
        QString uuid;
        QString encryption = "none";
        QString flow;
        std::shared_ptr<xrayStreamSetting> streamSetting = std::make_shared<xrayStreamSetting>();
        std::shared_ptr<xrayMultiplex> multiplex = std::make_shared<xrayMultiplex>();

        bool ParseFromLink(const QString& link) override;
        bool ParseFromJson(const QJsonObject& object) override;
        bool ParseFromClash(const clash::Proxies& object) override;
        QString ExportToLink() const override;
        QJsonObject ExportToJson() const override;
        BuildResult Build() const override;
        BuildResult BuildXray() const override;

        std::shared_ptr<xrayStreamSetting> GetXrayStream() const override { return streamSetting; }

        std::shared_ptr<xrayMultiplex> GetXrayMultiplex() const override { return multiplex; }

        QString DisplayType() const override {
            return "VLESS (Xray)";
        }
        bool IsXray() const override {
           return true;
        }
    };
}
