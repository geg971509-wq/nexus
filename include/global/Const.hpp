#pragma once
#include <QString>
#include <QStringList>

namespace Configs {
    namespace DomainMatcher {
        enum DomainMatcher {
            DEFAULT,
            MPH,
        };
    }

    namespace DomainStrategy {
        inline QStringList DomainStrategy = {"", "ipv4_only", "ipv6_only", "prefer_ipv4", "prefer_ipv6"};
    }

    namespace SingboxOptions {
        inline QStringList SniffProtocols = {"http", "tls", "quic", "stun", "dns", "bittorrent", "dtls", "ssh", "rdp"};
        inline QStringList ActionTypes = {"route", "reject", "hijack-dns", "route-options", "sniff", "resolve"};
        inline QStringList rejectMethods = {"default", "drop", "reply"};
    }

    namespace CoreType {
        enum CoreType {
            SING_BOX,
        };
    }

    namespace Information {
        inline QStringList iconNames = {"Dns.png", "Off.png", "Proxy.png", "Proxy-Dns.png", "Throne.png", "Tun.png"};
    }

    namespace TestConfig
    {
        enum SpeedTestMode
        {
            FULL,
            DL,
            UL,
            SIMPLEDL,
            COUNTRY,
        };
    }

    namespace Mirrors
    {
        enum Mirrors
        {
            GITHUB,
            CLOUDFLARE,
            GCORE,
            QUANTIL,
            FASTLY,
            CDN,
        };
    }

    namespace VPNImplementation {
        inline QStringList VPNImplementation = {"system", "gvisor", "mixed"};
    }

    namespace Xray {
        inline QStringList XrayLogLevels = {"debug", "info", "warning", "error", "none"};
        inline QStringList XrayVlessPreferenceString = {"XHTTP Only", "XHTTP And Reality", "All VLESS"};
        enum XrayVlessPreference {
            XhttpOnly,
            XhttpAndReality,
            AllVLESS,
        };
    }
} // namespace Configs
