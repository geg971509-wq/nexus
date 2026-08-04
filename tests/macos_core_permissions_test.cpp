#include <QFile>
#include <QString>

#include <cstdlib>

#ifndef THRONE_SOURCE_DIR
#error THRONE_SOURCE_DIR must be defined
#endif

namespace {
QString readSource() {
    QFile file(QStringLiteral(THRONE_SOURCE_DIR "/src/sys/macos/MacOS.cpp"));
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) return {};
    return QString::fromUtf8(file.readAll());
}
}

int main() {
    const auto source = readSource();
    if (source.isEmpty()) return EXIT_FAILURE;

    const auto printfPath = QStringLiteral(" && /usr/bin/printf ");
    const auto obsoletePath = QStringLiteral(" && /bin/printf ");
    if (!source.contains(printfPath) || source.contains(obsoletePath)) {
        return EXIT_FAILURE;
    }

    return EXIT_SUCCESS;
}
