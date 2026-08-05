#pragma once
#include "include/configs/baseConfig.h"

namespace Configs
{
    inline QStringList muxProtocols = {"smux", "yamux", "h2mux"};

    // Per-profile mux probe result. Default/unknown must not enable mux via mux_default_on.
    enum class MuxCapability : int {
        Unknown = 0,
        Yes = 1,
        No = 2,
    };

    class TcpBrutal : public baseConfig
    {
    public:
        bool enabled = false;
        int up_mbps = 0;
        int down_mbps = 0;

        // baseConfig overrides
        bool ParseFromLink(const QString& link) override;
        bool ParseFromJson(const QJsonObject& object) override;
        QString ExportToLink() const override;
        QJsonObject ExportToJson() const override;
        BuildResult Build() const override;
    };

    class Multiplex : public baseConfig
    {
        public:
        bool enabled = false;
        bool unspecified = true; // tri-state defaults to "Keep Default"
        QString protocol;
        int max_connections = 0;
        int min_streams = 0;
        int max_streams = 0;
        bool padding = false;
        std::shared_ptr<TcpBrutal> brutal = std::make_shared<TcpBrutal>();

        int getMuxState() {
            if (enabled) return 1;
            if (!unspecified) return 2;
            return 0;
        }

        void saveMuxState(int state) {
            unspecified = false;
            if (state == 1) {
                enabled = true;
                return;
            }
            enabled = false;
            if (state == 0) unspecified = true;
        }

        // baseConfig overrides
        bool ParseFromLink(const QString& link) override;
        bool ParseFromJson(const QJsonObject& object) override;
        bool ParseFromClash(const clash::Proxies& object) override;
        QString ExportToLink() const override;
        QJsonObject ExportToJson() const override;
        // Override: capability Unknown (safe default-on gate).
        BuildResult Build() const override;
        // cap: only used when unspecified && mux_default_on — enable only if Yes.
        BuildResult Build(MuxCapability cap) const;
    };
}
