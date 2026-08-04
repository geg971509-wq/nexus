#pragma once
#include <QHostInfo>
#include <utility>
#include "DialFields.h"
#include "multiplex.h"
#include "TLS.h"
#include "transport.h"
#include "xrayMultiplex.h"
#include "xrayStreamSetting.h"
#include "include/configs/baseConfig.h"

namespace Configs
{
    inline QStringList vPacketEncoding = {"", "packetaddr", "xudp"};

    // Ordered worst-to-best; the ordering is relied on when sorting by security.
    enum class SecurityLevel {
        Unknown = 0,
        None,
        Weak,
        Secure,
    };

    struct SecurityInfo {
        QString label;
        QString transport;
        SecurityLevel level = SecurityLevel::Unknown;

        bool isDangerous() const {
            return level == SecurityLevel::None || level == SecurityLevel::Weak;
        }
    };

    // Empty for the transports not worth showing (plain tcp / xray raw).
    QString DisplayTransportName(const QString& type);

    class outbound : public baseConfig
    {
    public:
        QString name;
        QString server;
        int server_port = 0;
        bool invalid = false;
        std::shared_ptr<DialFields> dialFields = std::make_shared<DialFields>();

        void ResolveDomainToIP(const std::function<void()> &onFinished) {
            bool noResolve = false;
            auto serverAddr = GetAddress();
            if (IsIpAddress(serverAddr) || serverAddr.isEmpty()) noResolve = true;
            if (noResolve) {
                onFinished();
                return;
            }
            QHostInfo::lookupHost(serverAddr, QApplication::instance(), [=, this](const QHostInfo &host) {
                auto addrs = host.addresses();
                if (!addrs.isEmpty()) SetAddress(addrs.first().toString());
                onFinished();
            });
        }

        virtual void SetAddress(QString newAddr) {
            server = std::move(newAddr);
        }

        virtual QString GetAddress() const
        {
            return server;
        }

        virtual void SetPort(int newPort) {
            server_port = newPort;
        }

        virtual QString GetPort() const {
            return QString::number(server_port);
        }

        virtual QString DisplayAddress() const
        {
            return ::DisplayAddress(server, server_port);
        }

        virtual QString DisplayName() const
        {
            if (name.isEmpty()) {
                return DisplayAddress();
            }
            return name;
        }

        virtual QString DisplayType() const { return {}; };

        QString DisplayTypeAndName() const
        {
            return QString("[%1] %2").arg(DisplayType(), DisplayName());
        }

        // Overridden by protocols with their own crypto (WireGuard, SSH, ...);
        // the default reads TLS/transport (or the Xray stream settings).
        virtual SecurityInfo GetSecurity() const;

        // GetSecurity() rendered for the type column, warning-prefixed when weak.
        QString DisplaySecurity() const;

    protected:
        SecurityInfo SecurityFromTLS(const QString& transport) const;

    public:

        virtual bool IsXray() const { return false; }

        virtual bool IsExtraCore() const { return false; }

        virtual bool IsXrayFullConfig() const { return false; }

        virtual bool HasMux() const { return false; }

        virtual bool HasTransport() const { return false; }

        virtual bool HasTLS() const { return false; }

        virtual bool MustTLS() const { return false; }

        virtual std::shared_ptr<TLS> GetTLS() const { return std::make_shared<TLS>(); }

        virtual std::shared_ptr<Transport> GetTransport() const { return std::make_shared<Transport>(); }

        virtual std::shared_ptr<Multiplex> GetMux() const { return std::make_shared<Multiplex>(); }

        virtual std::shared_ptr<xrayStreamSetting> GetXrayStream() const { return std::make_shared<xrayStreamSetting>(); }

        virtual std::shared_ptr<xrayMultiplex> GetXrayMultiplex() const { return std::make_shared<xrayMultiplex>(); }

        virtual bool IsEndpoint() const { return false; };

        virtual BuildResult BuildXray() const { return {}; }

        QString ExportJsonLink(bool stripMetadata = false) const {
            auto json = ExportToJson();
            if (stripMetadata) {
                json.remove("tag");
            }
            const auto b64 = QJsonObject2QString(json, true)
                                .toUtf8()
                                .toBase64(QByteArray::Base64UrlEncoding | QByteArray::OmitTrailingEquals);
            return QStringLiteral("throne://add/") + QString::fromLatin1(b64);
        }

        virtual QJsonObject ExportToStorageJson() const {
            return ExportToJson();
        }

        // baseConfig overrides
        bool ParseFromLink(const QString& link) override;
        bool ParseFromJson(const QJsonObject& object) override;
        bool ParseFromClash(const clash::Proxies& object) override;
        QString ExportToLink() const override;
        QJsonObject ExportToJson() const override;
        QJsonObject ExportIdentity() const override;
        BuildResult Build() const override;
    };
}
