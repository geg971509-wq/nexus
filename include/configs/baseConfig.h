#pragma once

#include <QJsonObject>
#include "include/global/Configs.hpp"
#include "include/configs/sub/clash.hpp"

namespace Configs
{
    struct BuildResult {
        QJsonObject object;
        QString error;
    };

    class baseConfig
    {
    public:
        virtual ~baseConfig() = default;

        [[nodiscard]] virtual bool ParseFromLink(const QString& link) {
            return false;
        }

        [[nodiscard]] virtual bool ParseFromJson(const QJsonObject& object) {
            return false;
        }

        [[nodiscard]] virtual bool ParseFromClash(const clash::Proxies& object) {
            return false;
        }

        [[nodiscard]] virtual QString ExportToLink() const {
            return {};
        }

        virtual QJsonObject ExportToJson() const {
            return {};
        }

        virtual QJsonObject ExportIdentity() const {
            return ExportToJson();
        }

        [[nodiscard]] virtual BuildResult Build() const {
            return {{}, "base class function called!"};
        }
    };
}
