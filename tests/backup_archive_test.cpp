#include "include/ui/setting/BackupArchive.h"

#include <QBuffer>

#include <cstdlib>

namespace {

QByteArray validArchive() {
    QByteArray bytes;
    QBuffer buffer(&bytes);
    if (!buffer.open(QIODevice::WriteOnly)) return {};

    QJsonObject metadata{
        {QStringLiteral("created_at"), QStringLiteral("today")},
        {QStringLiteral("parts"),
         QJsonObject{
             {QStringLiteral("profiles"), true},
             {QStringLiteral("routes"), true},
             {QStringLiteral("settings"), true},
             {QStringLiteral("icons"), true},
         }},
    };
    QMap<QString, QByteArray> files{
        {QStringLiteral("database"), QByteArrayLiteral("db")},
        {QString::fromUtf8("icons/图标.png"), QByteArrayLiteral("icon")},
    };
    QString error;
    if (!BackupArchive::write(buffer, metadata, files, &error)) return {};
    return bytes;
}

bool readsValidArchive() {
    QByteArray bytes = validArchive();
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::ReadOnly);

    BackupArchive::Archive archive;
    QString error;
    BackupArchive::Parts parts;
    return BackupArchive::read(buffer, &archive, &error)
        && archive.formatVersion == BackupArchive::FormatVersion
        && archive.metadata.value("created_at").toString() == "today"
        && archive.files.value("database") == "db"
        && archive.files.value(QString::fromUtf8("icons/图标.png")) == "icon"
        && BackupArchive::partsForRestore(archive, &parts, &error)
        && parts.profiles && parts.icons;
}

bool readsLegacyArchive() {
    QByteArray bytes;
    QBuffer writer(&bytes);
    writer.open(QIODevice::WriteOnly);
    QDataStream stream(&writer);
    stream.setByteOrder(QDataStream::LittleEndian);
    stream.setVersion(QDataStream::Qt_6_0);
    stream.writeRawData("THRN", 4);
    stream << quint32{1}
           << QString::fromUtf8(QJsonDocument(QJsonObject{{"created_at", "legacy"}})
                                    .toJson(QJsonDocument::Compact))
           << QMap<QString, QByteArray>{{QStringLiteral("database"), QByteArrayLiteral("db")}};
    writer.close();

    QBuffer reader(&bytes);
    reader.open(QIODevice::ReadOnly);
    BackupArchive::Archive archive;
    QString error;
    BackupArchive::Parts parts;
    return BackupArchive::read(reader, &archive, &error)
        && archive.formatVersion == 1 && archive.files.value("database") == "db"
        && BackupArchive::partsForRestore(archive, &parts, &error)
        && parts.profiles && parts.routes && parts.settings && !parts.icons;
}

bool rejectsOversizedIcon() {
    QByteArray bytes;
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::WriteOnly);
    QMap<QString, QByteArray> files{
        {QStringLiteral("icons/large.png"), QByteArray(BackupArchive::MaxIconBytes + 1, 'x')},
    };
    QString error;
    return !BackupArchive::write(buffer, {}, files, &error);
}

bool rejectsForgedLength() {
    QByteArray bytes;
    QBuffer writer(&bytes);
    writer.open(QIODevice::WriteOnly);
    QDataStream stream(&writer);
    stream.setByteOrder(QDataStream::LittleEndian);
    stream.setVersion(QDataStream::Qt_6_0);
    stream.writeRawData("THRN", 4);
    stream << BackupArchive::FormatVersion
           << static_cast<quint32>(BackupArchive::MaxMetadataBytes + 1);
    writer.close();

    QBuffer reader(&bytes);
    reader.open(QIODevice::ReadOnly);
    BackupArchive::Archive archive;
    QString error;
    return !BackupArchive::read(reader, &archive, &error);
}

bool rejectsTrailingData() {
    QByteArray bytes = validArchive();
    bytes += 'x';
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::ReadOnly);

    BackupArchive::Archive archive;
    QString error;
    return !BackupArchive::read(buffer, &archive, &error);
}

bool rejectsPathTraversal() {
    QString error;
    QMap<QString, qint64> entries{{QStringLiteral("icons/../evil.png"), 1}};
    return BackupArchive::portableIconKey(QStringLiteral("icons/../evil.png")).isEmpty()
        && BackupArchive::portableIconKey(QStringLiteral("icons/CON.png")).isEmpty()
        && !BackupArchive::validateEntrySizes(entries, 0, &error);
}

bool rejectsCaseFoldCollision() {
    QString error;
    QMap<QString, qint64> entries{
        {QStringLiteral("icons/Icon.png"), 1},
        {QStringLiteral("icons/icon.png"), 1},
    };
    return !BackupArchive::validateEntrySizes(entries, 0, &error);
}

bool rejectsTooManyEntries() {
    QString error;
    QMap<QString, qint64> entries;
    for (quint32 i = 0; i < BackupArchive::MaxEntries + 1; ++i)
        entries.insert(QStringLiteral("icons/i%1.png").arg(i), 1);
    return !BackupArchive::validateEntrySizes(entries, 0, &error);
}

bool rejectsUnknownEntry() {
    QString error;
    QMap<QString, qint64> entries{{QStringLiteral("secrets"), 1}};
    return !BackupArchive::validateEntrySizes(entries, 0, &error);
}

bool acceptsExactIconLimit() {
    QString error;
    QMap<QString, qint64> entries{
        {QStringLiteral("icons/exact.png"), BackupArchive::MaxIconBytes},
    };
    return BackupArchive::validateEntrySizes(entries, 0, &error);
}

} // namespace

int main() {
    return readsValidArchive() && readsLegacyArchive() && rejectsOversizedIcon()
            && rejectsForgedLength() && rejectsTrailingData() && rejectsPathTraversal()
            && rejectsCaseFoldCollision() && rejectsTooManyEntries() && rejectsUnknownEntry()
            && acceptsExactIconLimit()
        ? EXIT_SUCCESS
        : EXIT_FAILURE;
}
