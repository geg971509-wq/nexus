// Covers Configs::parseIntOr, which reads ints out of untrusted share links.
// A non-numeric value must keep the field's existing default rather than
// collapsing it to 0, while every numeric value must match plain toInt().

#include "include/configs/common/utils.h"

#include <QString>
#include <QUrlQuery>

#include <cstdlib>

namespace {
    bool numericMatchesToInt() {
        const char *values[] = {"0", "1", "443", "1280", "-5", "2147483647", "-2147483648"};
        for (const auto *v : values) {
            const QString s = QString::fromLatin1(v);
            if (Configs::parseIntOr(s, 9999) != s.toInt()) return false;
        }
        return true;
    }

    bool junkKeepsFallback() {
        // Each of these makes plain toInt() return 0; parseIntOr must not.
        const char *junk[] = {"abc", "", " ", "1280abc", "12.5", "0x10",
                              "99999999999999999999", "--3", "+", "1 2"};
        for (const auto *j : junk) {
            const QString s = QString::fromLatin1(j);
            if (s.toInt() != 0) return false;                    // premise of the fix
            if (Configs::parseIntOr(s, 1420) != 1420) return false;
        }
        return true;
    }

    bool callShapeUsedByParsers() {
        const QUrlQuery query(QStringLiteral("mtu=1280&bad=abc&zero=0"));

        int mtu = 1420;
        if (query.hasQueryItem("mtu")) mtu = Configs::parseIntOr(query.queryItemValue("mtu"), mtu);
        if (mtu != 1280) return false;

        int broken = 1420;
        if (query.hasQueryItem("bad")) broken = Configs::parseIntOr(query.queryItemValue("bad"), broken);
        if (broken != 1420) return false;                        // preserved, not zeroed

        int explicitZero = 1420;
        if (query.hasQueryItem("zero"))
            explicitZero = Configs::parseIntOr(query.queryItemValue("zero"), explicitZero);
        if (explicitZero != 0) return false;                     // a real 0 still wins

        int absent = 1420;
        if (query.hasQueryItem("missing"))
            absent = Configs::parseIntOr(query.queryItemValue("missing"), absent);
        return absent == 1420;
    }

    bool okOutReportsParse() {
        bool ok = false;
        if (Configs::parseIntOr("443", 1420, &ok) != 443 || !ok) return false;
        ok = true;
        if (Configs::parseIntOr("abc", 1420, &ok) != 1420 || ok) return false;
        ok = true;
        if (Configs::parseIntOr("", 1420, &ok) != 1420 || ok) return false;
        return true;
    }

    bool optMatchesToInt() {
        const char *values[] = {"0", "1", "443", "1280", "-5", "2147483647", "-2147483648"};
        for (const auto *v : values) {
            const QString s = QString::fromLatin1(v);
            const auto parsed = Configs::parseIntOpt(s);
            if (!parsed.has_value() || *parsed != s.toInt()) return false;
        }
        return true;
    }

    bool optRejectsJunk() {
        // Same junk set as junkKeepsFallback: parseIntOpt must disengage.
        const char *junk[] = {"abc", "", " ", "1280abc", "12.5", "0x10",
                              "99999999999999999999", "--3", "+", "1 2"};
        for (const auto *j : junk) {
            if (Configs::parseIntOpt(QString::fromLatin1(j)).has_value()) return false;
        }
        return true;
    }

    bool optDrivesEnableOnSuccess() {
        // The amnezia field-chain shape: enable only when the value parses.
        bool enable = false;
        int jc = 0;
        if (const auto v = Configs::parseIntOpt(QStringLiteral("7"))) {
            jc = *v;
            enable = true;
        }
        if (!enable || jc != 7) return false;
        enable = false;
        if (const auto v = Configs::parseIntOpt(QStringLiteral("junk"))) {
            jc = *v;
            enable = true;
        }
        return !enable && jc == 7;
    }
}

int main() {
    if (!numericMatchesToInt()) return EXIT_FAILURE;
    if (!junkKeepsFallback()) return EXIT_FAILURE;
    if (!callShapeUsedByParsers()) return EXIT_FAILURE;
    if (!okOutReportsParse()) return EXIT_FAILURE;
    if (!optMatchesToInt()) return EXIT_FAILURE;
    if (!optRejectsJunk()) return EXIT_FAILURE;
    if (!optDrivesEnableOnSuccess()) return EXIT_FAILURE;
    return EXIT_SUCCESS;
}
