#include "include/global/CountryHelper.hpp"

static bool isIsoCountryCode(const QString& code) {
    if (code.size() != 2) return false;
    const QChar a = code[0], b = code[1];
    return a.isUpper() && b.isUpper() && a.isLetter() && b.isLetter();
}

QString CountryNameToCode(const QString& countryName) {
    if (countryName.isEmpty()) return {};
    const QString trimmed = countryName.trimmed();
    if (isIsoCountryCode(trimmed.toUpper()) && trimmed.size() == 2)
        return trimmed.toUpper();
    // Exact English name first.
    if (auto it = CountryMap.constFind(trimmed); it != CountryMap.constEnd())
        return it.value();
    // Case-insensitive name match for speedtest / provider variants.
    for (auto it = CountryMap.constBegin(); it != CountryMap.constEnd(); ++it) {
        if (it.key().compare(trimmed, Qt::CaseInsensitive) == 0)
            return it.value();
    }
    return {};
}

// Common non-ISO 2-letter tags used in subscription names → ISO 3166-1 alpha-2.
// "UK" is not a valid regional-indicator flag pair; real flag needs "GB".
static QString normalizeIsoCountryCode(const QString& code) {
    if (code == QLatin1String("UK")) return QStringLiteral("GB");
    return code;
}

QString InferCountryCode(const QString& text) {
    if (text.isEmpty()) return {};
    const QString trimmed = text.trimmed();
    // Already a code, or a full country name.
    if (QString code = CountryNameToCode(trimmed); !code.isEmpty())
        return normalizeIsoCountryCode(code);
    // Profile / subscription name prefix: "TW - 台北...", "HK-...", "UK - 英国..."
    // Take the first 2-letter token before common separators.
    static const QList<QChar> seps = {
        QChar('-'), QChar('|'), QChar('/'),
        QChar(0x00B7), // ·
        QChar(0x2014), // —
        QChar(0x2013), // –
    };
    int cut = trimmed.size();
    for (QChar s : seps) {
        int i = trimmed.indexOf(s);
        if (i > 0 && i < cut) cut = i;
    }
    QString head = trimmed.left(cut).trimmed();
    // Also split on whitespace if the head is still long ("HK BGP").
    const int sp = head.indexOf(QChar(' '));
    if (sp > 0) head = head.left(sp).trimmed();
    head = head.toUpper();
    if (isIsoCountryCode(head)) return normalizeIsoCountryCode(head);
    return {};
}

QString CountryCodeToFlag(const QString& countryCode) {
    const QString code = InferCountryCode(countryCode);
    if (code.size() != 2) return {};
    // Regional Indicator Symbol Letter A is U+1F1E6; ISO letter 'A' is 0x41.
    // offset = 0x1F1E6 - 'A' = 0x1F1A5.
    QVector<uint> ucs4 = code.toUcs4();
    for (uint& c : ucs4) c += 0x1F1A5;
    return QString::fromUcs4(reinterpret_cast<const char32_t *>(ucs4.constData()), code.size());
}
