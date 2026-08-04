#include "include/configs/TunPrivateBypass.hpp"

#include <cstdlib>
#include <QSet>
#include <QString>

using Configs::TunPrivateBypass::privateRangeBypassExcludingTun;
using Configs::TunPrivateBypass::privateRangeBypassExcludingTunV6;
using Configs::TunPrivateBypass::expectedTunSystemDnsV4;
using Configs::TunPrivateBypass::isPrivateSiteIPv4;
using Configs::TunPrivateBypass::parseV4Cidr;
using Configs::TunPrivateBypass::V4Cidr;

static QSet<QString> toSet(const QJsonArray& arr) {
    QSet<QString> out;
    for (const auto& v : arr) out.insert(v.toString());
    return out;
}

int main() {
    // Default Tun 172.19.0.1/24 must be carved out of 172.16.0.0/12.
    {
        const auto arr = privateRangeBypassExcludingTun("172.19.0.1/24");
        const auto set = toSet(arr);
        if (set.contains("172.16.0.0/12")) return EXIT_FAILURE;
        if (set.contains("172.19.0.0/24")) return EXIT_FAILURE;
        // Siblings covering the rest of 172.16/12 around 172.19.0.0/24.
        if (!set.contains("172.16.0.0/15")) return EXIT_FAILURE;   // 172.16-17
        if (!set.contains("172.18.0.0/16")) return EXIT_FAILURE;   // 172.18
        if (!set.contains("172.19.1.0/24")) return EXIT_FAILURE;   // after hole
        if (!set.contains("172.19.128.0/17")) return EXIT_FAILURE;
        if (!set.contains("172.19.64.0/18")) return EXIT_FAILURE;
        if (!set.contains("172.19.32.0/19")) return EXIT_FAILURE;
        if (!set.contains("172.19.16.0/20")) return EXIT_FAILURE;
        if (!set.contains("172.19.8.0/21")) return EXIT_FAILURE;
        if (!set.contains("172.19.4.0/22")) return EXIT_FAILURE;
        if (!set.contains("172.19.2.0/23")) return EXIT_FAILURE;
        if (!set.contains("172.20.0.0/14")) return EXIT_FAILURE;   // 172.20-23
        if (!set.contains("172.24.0.0/13")) return EXIT_FAILURE;   // 172.24-31
        // Untouched private ranges stay whole.
        if (!set.contains("10.0.0.0/8")) return EXIT_FAILURE;
        if (!set.contains("192.168.0.0/16")) return EXIT_FAILURE;
    }

    // System DNS is Tun host + 1.
    if (expectedTunSystemDnsV4("172.19.0.1/24") != "172.19.0.2") return EXIT_FAILURE;

    // Overlap helper for UI hint.
    {
        V4Cidr c{};
        if (!parseV4Cidr("172.19.0.1/24", c) || !isPrivateSiteIPv4(c)) return EXIT_FAILURE;
        if (!parseV4Cidr("198.18.0.1/24", c) || isPrivateSiteIPv4(c)) return EXIT_FAILURE;
    }

    // Non-overlapping Tun CIDR leaves private list intact.
    {
        const auto set = toSet(privateRangeBypassExcludingTun("198.18.0.1/24"));
        if (!set.contains("172.16.0.0/12")) return EXIT_FAILURE;
    }

    // IPv6: default ULA Tun is carved out of fc00::/7.
    {
        const auto set = toSet(privateRangeBypassExcludingTunV6("fdfe:dcba:9876::1/96"));
        if (set.contains("fc00::/7")) return EXIT_FAILURE;
        if (set.contains("fdfe:dcba:9876::/96")) return EXIT_FAILURE;
        if (!set.contains("fe80::/10")) return EXIT_FAILURE;
        if (!set.contains("::1/128")) return EXIT_FAILURE;
    }

    return EXIT_SUCCESS;
}
