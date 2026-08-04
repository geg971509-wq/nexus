#include "include/global/Version.hpp"

#include <QVersionNumber>

#include <optional>
#include <utility>

namespace {

enum class Prerelease {
    Alpha,
    Beta,
    Rc,
    Stable,
};

struct ParsedVersion {
    QVersionNumber number;
    Prerelease prerelease;
    int revision;
};

std::optional<ParsedVersion> ParseVersion(const QString &text) {
    qsizetype suffixIndex = 0;
    auto number = QVersionNumber::fromString(text, &suffixIndex);
    if (number.segmentCount() != 3) return std::nullopt;
    if (suffixIndex == text.size()) return ParsedVersion{std::move(number), Prerelease::Stable, 0};
    if (text[suffixIndex] != '-') return std::nullopt;

    const auto parts = text.sliced(suffixIndex + 1).split('.');
    if (parts.size() != 2) return std::nullopt;

    bool ok = false;
    const auto revision = parts[1].toInt(&ok);
    if (!ok || revision < 0) return std::nullopt;

    auto prerelease = Prerelease::Stable;
    if (parts[0] == "alpha") prerelease = Prerelease::Alpha;
    else if (parts[0] == "beta") prerelease = Prerelease::Beta;
    else if (parts[0] == "rc") prerelease = Prerelease::Rc;
    else return std::nullopt;
    return ParsedVersion{std::move(number), prerelease, revision};
}

}

bool Throne::IsVersionNewer(const QString &candidateText, const QString &currentText) {
    const auto candidate = ParseVersion(candidateText);
    const auto current = ParseVersion(currentText);
    if (!candidate || !current) return false;

    const auto numberComparison = QVersionNumber::compare(candidate->number, current->number);
    if (numberComparison != 0) return numberComparison > 0;
    if (candidate->prerelease != current->prerelease) return candidate->prerelease > current->prerelease;
    return candidate->revision > current->revision;
}
