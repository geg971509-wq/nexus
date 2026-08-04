#pragma once

#include <QAbstractSocket>
#include <QHostAddress>
#include <QJsonArray>
#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <array>

// Tun private-bypass carve helpers. sing-tun treats route_address as an allowlist
// (replaces full auto_route), so Tun must be carved out of route_exclude_address
// instead of "pinned" back via route_address. Core system DNS is Tun address + 1.

namespace Configs {
namespace TunPrivateBypass {

struct V4Cidr {
    quint32 network = 0;
    int bits = -1;
};

inline bool parseV4Cidr(const QString& text, V4Cidr& out) {
    const auto parts = text.split('/');
    if (parts.size() != 2) return false;
    const QHostAddress addr(parts[0]);
    bool ok = false;
    const int bits = parts[1].toInt(&ok);
    if (!ok || bits < 0 || bits > 32 || addr.protocol() != QAbstractSocket::IPv4Protocol)
        return false;
    const quint32 host = addr.toIPv4Address();
    const quint32 mask = bits == 0 ? 0u : (0xFFFFFFFFu << (32 - bits));
    out.network = host & mask;
    out.bits = bits;
    return true;
}

inline QString formatV4Cidr(quint32 network, int bits) {
    return QHostAddress(network).toString() + '/' + QString::number(bits);
}

inline bool cidrContainsV4(const V4Cidr& outer, const V4Cidr& inner) {
    if (outer.bits < 0 || inner.bits < 0 || outer.bits > inner.bits) return false;
    const quint32 mask = outer.bits == 0 ? 0u : (0xFFFFFFFFu << (32 - outer.bits));
    return (inner.network & mask) == (outer.network & mask);
}

// base \ hole as a list of CIDRs. hole must be fully inside base.
inline QStringList subtractV4Cidr(const V4Cidr& base, const V4Cidr& hole) {
    QStringList out;
    if (!cidrContainsV4(base, hole)) {
        out << formatV4Cidr(base.network, base.bits);
        return out;
    }
    if (base.bits == hole.bits) return out;
    quint32 curNet = base.network;
    int curBits = base.bits;
    while (curBits < hole.bits) {
        ++curBits;
        const quint32 bit = 1u << (32 - curBits);
        const quint32 left = curNet;
        const quint32 right = curNet | bit;
        // Keep the half that still contains hole; emit the sibling.
        if ((hole.network & bit) == 0u) {
            out << formatV4Cidr(right, curBits);
            curNet = left;
        } else {
            out << formatV4Cidr(left, curBits);
            curNet = right;
        }
    }
    return out;
}

inline QStringList privateIPv4Ranges() {
    return {
        "127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16",
        "169.254.0.0/16", "224.0.0.0/4", "255.255.255.255/32"
    };
}

// Private LAN bypass list with Tun subnet removed so Core's system DNS
// (Tun address + 1) is never swallowed by 172.16.0.0/12 (or any other
// private range that happens to contain vpn_tun_ipv4_cidr).
inline QJsonArray privateRangeBypassExcludingTun(const QString& tunCidrText) {
    V4Cidr tun{};
    const bool haveTun = parseV4Cidr(tunCidrText, tun);
    QJsonArray out;
    for (const auto& rangeText : privateIPv4Ranges()) {
        V4Cidr range{};
        if (!haveTun || !parseV4Cidr(rangeText, range) || !cidrContainsV4(range, tun)) {
            out.append(rangeText);
            continue;
        }
        for (const auto& piece : subtractV4Cidr(range, tun)) out.append(piece);
    }
    return out;
}

inline bool isPrivateSiteIPv4(const V4Cidr& cidr) {
    static const V4Cidr ranges[] = {
        {0x0A000000u, 8},   // 10.0.0.0/8
        {0xAC100000u, 12},  // 172.16.0.0/12
        {0xC0A80000u, 16},  // 192.168.0.0/16
    };
    for (const auto& r : ranges) {
        if (cidrContainsV4(r, cidr)) return true;
    }
    return false;
}

// --- IPv6 (ULA / link-local / multicast / loopback) ---

struct V6Cidr {
    std::array<quint8, 16> network{};
    int bits = -1;
};

inline void maskV6(std::array<quint8, 16>& addr, int bits) {
    for (int i = 0; i < 16; ++i) {
        const int start = i * 8;
        if (bits <= start) {
            addr[i] = 0;
        } else if (bits < start + 8) {
            const int keep = bits - start;
            addr[i] &= static_cast<quint8>(0xFFu << (8 - keep));
        }
    }
}

inline bool parseV6Cidr(const QString& text, V6Cidr& out) {
    const auto parts = text.split('/');
    if (parts.size() != 2) return false;
    const QHostAddress addr(parts[0]);
    bool ok = false;
    const int bits = parts[1].toInt(&ok);
    if (!ok || bits < 0 || bits > 128 || addr.protocol() != QAbstractSocket::IPv6Protocol)
        return false;
    const auto a = addr.toIPv6Address();
    for (int i = 0; i < 16; ++i) out.network[i] = a[i];
    maskV6(out.network, bits);
    out.bits = bits;
    return true;
}

inline QString formatV6Cidr(const std::array<quint8, 16>& network, int bits) {
    Q_IPV6ADDR a{};
    for (int i = 0; i < 16; ++i) a[i] = network[i];
    return QHostAddress(a).toString() + '/' + QString::number(bits);
}

inline bool cidrContainsV6(const V6Cidr& outer, const V6Cidr& inner) {
    if (outer.bits < 0 || inner.bits < 0 || outer.bits > inner.bits) return false;
    auto outerMasked = outer.network;
    auto innerMasked = inner.network;
    maskV6(outerMasked, outer.bits);
    maskV6(innerMasked, outer.bits);
    return outerMasked == innerMasked;
}

inline bool bitAtV6(const std::array<quint8, 16>& addr, int bitIndex1Based) {
    // bitIndex1Based: 1 = MSB of first byte, 128 = LSB of last byte
    const int idx = bitIndex1Based - 1;
    const int byte = idx / 8;
    const int bit = 7 - (idx % 8);
    return (addr[byte] >> bit) & 1;
}

inline void setBitV6(std::array<quint8, 16>& addr, int bitIndex1Based) {
    const int idx = bitIndex1Based - 1;
    const int byte = idx / 8;
    const int bit = 7 - (idx % 8);
    addr[byte] |= static_cast<quint8>(1u << bit);
}

inline QStringList subtractV6Cidr(const V6Cidr& base, const V6Cidr& hole) {
    QStringList out;
    if (!cidrContainsV6(base, hole)) {
        out << formatV6Cidr(base.network, base.bits);
        return out;
    }
    if (base.bits == hole.bits) return out;
    auto curNet = base.network;
    int curBits = base.bits;
    while (curBits < hole.bits) {
        ++curBits;
        auto left = curNet;
        auto right = curNet;
        setBitV6(right, curBits);
        maskV6(left, curBits);
        maskV6(right, curBits);
        if (!bitAtV6(hole.network, curBits)) {
            out << formatV6Cidr(right, curBits);
            curNet = left;
        } else {
            out << formatV6Cidr(left, curBits);
            curNet = right;
        }
    }
    return out;
}

inline QStringList privateIPv6Ranges() {
    return {
        "::1/128",
        "fc00::/7",
        "fe80::/10",
        "ff00::/8",
    };
}

inline QJsonArray privateRangeBypassExcludingTunV6(const QString& tunCidrText) {
    V6Cidr tun{};
    const bool haveTun = parseV6Cidr(tunCidrText, tun);
    QJsonArray out;
    for (const auto& rangeText : privateIPv6Ranges()) {
        V6Cidr range{};
        if (!haveTun || !parseV6Cidr(rangeText, range) || !cidrContainsV6(range, tun)) {
            out.append(rangeText);
            continue;
        }
        for (const auto& piece : subtractV6Cidr(range, tun)) out.append(piece);
    }
    return out;
}

// Host address from "addr/prefix" (no mask applied to the host itself).
inline QString cidrHostAddress(const QString& cidrText) {
    const auto parts = cidrText.split('/');
    if (parts.isEmpty()) return {};
    return parts[0].trimmed();
}

// Core sets system DNS to Tun IPv4 address + 1 (next).
inline QString expectedTunSystemDnsV4(const QString& tunCidrText) {
    V4Cidr tun{};
    if (!parseV4Cidr(tunCidrText, tun)) return {};
    const QHostAddress host(tunCidrText.split('/').value(0));
    if (host.protocol() != QAbstractSocket::IPv4Protocol) return {};
    return QHostAddress(host.toIPv4Address() + 1).toString();
}

} // namespace TunPrivateBypass
} // namespace Configs
