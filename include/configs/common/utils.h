#pragma once
#include <QJsonObject>
#include <QUrlQuery>

#include <optional>

namespace Configs
{
    // Parse an int out of an untrusted link/config value; std::nullopt when
    // the value is not a valid int (instead of collapsing to 0 like toInt()).
    inline std::optional<int> parseIntOpt(const QString& s) {
        bool ok = false;
        const int v = s.toInt(&ok);
        if (!ok) return std::nullopt;
        return v;
    }

    // Parse an int out of an untrusted link/config value. A non-numeric value
    // keeps `fallback` instead of silently collapsing the field to 0.
    // `okOut`, when given, reports whether the value actually parsed.
    inline int parseIntOr(const QString& s, int fallback, bool* okOut = nullptr) {
        const auto v = parseIntOpt(s);
        if (okOut) *okOut = v.has_value();
        return v.value_or(fallback);
    }

    void mergeUrlQuery(QUrlQuery& baseQuery, const QString& strQuery);

    void mergeJsonObjects(QJsonObject& baseObject, const QJsonObject& obj);

    QStringList jsonObjectToQStringList(const QJsonObject& obj);

    QJsonObject qStringListToJsonObject(const QStringList& list);

    bool useXrayVless(const QString& link);

    QString getHeadersString(QStringList headers);

    QStringList parseHeaderPairs(const QString& rawHeader);
}
