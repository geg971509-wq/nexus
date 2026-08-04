#pragma once

#include <QByteArray>
#include <QByteArrayView>
#include <QDataStream>
#include <QFileInfo>
#include <QIODevice>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMap>
#include <QSet>
#include <QString>

#include <limits>
#include <utility>

namespace BackupArchive {

inline constexpr quint32 FormatVersion = 2;
inline constexpr qint64 MaxArchiveBytes = 256LL * 1024 * 1024;
inline constexpr qint64 MaxMetadataBytes = 64LL * 1024;
inline constexpr qint64 MaxDatabaseBytes = 256LL * 1024 * 1024;
inline constexpr qint64 MaxIconBytes = 16LL * 1024 * 1024;
inline constexpr qint64 MaxEntryNameBytes = 2048;
inline constexpr quint32 MaxEntries = 4096;

struct Archive {
    quint32 formatVersion = 0;
    QJsonObject metadata;
    QMap<QString, QByteArray> files;
};

struct Parts {
    bool profiles = false;
    bool routes = false;
    bool settings = false;
    bool icons = false;

    [[nodiscard]] bool anyDatabase() const { return profiles || routes || settings; }
    [[nodiscard]] bool any() const { return anyDatabase() || icons; }
};

inline bool fail(QString* error, const QString& message) {
    if (error) *error = message;
    return false;
}

inline bool readSize(QDataStream& stream, qint64 limit, qint64* size, QString* error,
                     bool allowNull = false) {
    quint32 encoded = 0;
    stream >> encoded;
    if (stream.status() != QDataStream::Ok)
        return fail(error, QStringLiteral("Unexpected end of archive"));
    if (encoded == std::numeric_limits<quint32>::max()) {
        if (!allowNull) return fail(error, QStringLiteral("Invalid null length"));
        *size = -1;
        return true;
    }
    if (encoded > static_cast<quint64>(limit))
        return fail(error, QStringLiteral("Archive field exceeds its size limit"));
    *size = encoded;
    return true;
}

inline bool readString(QDataStream& stream, qint64 limit, QString* value, QString* error) {
    qint64 bytes = 0;
    if (!readSize(stream, limit, &bytes, error) || (bytes & 1))
        return bytes & 1 ? fail(error, QStringLiteral("Invalid string length")) : false;
    if (!stream.device() || stream.device()->bytesAvailable() < bytes)
        return fail(error, QStringLiteral("Truncated archive string"));

    value->resize(bytes / 2);
    for (qsizetype i = 0; i < value->size(); ++i) {
        quint16 character = 0;
        stream >> character;
        if (stream.status() != QDataStream::Ok)
            return fail(error, QStringLiteral("Truncated archive string"));
        (*value)[i] = QChar(character);
    }
    return true;
}

inline bool readBytes(QDataStream& stream, qint64 limit, QByteArray* value, QString* error) {
    qint64 bytes = 0;
    if (!readSize(stream, limit, &bytes, error, true)) return false;
    if (bytes < 0) {
        *value = {};
        return true;
    }
    if (!stream.device() || stream.device()->bytesAvailable() < bytes)
        return fail(error, QStringLiteral("Truncated archive entry"));

    value->resize(bytes);
    if (bytes > 0 && stream.readRawData(value->data(), bytes) != bytes)
        return fail(error, QStringLiteral("Truncated archive entry"));
    return true;
}

inline QString portableIconKey(const QString& key) {
    if (!key.startsWith(QStringLiteral("icons/"))) return {};
    const QString name = key.mid(6);
    const QFileInfo info(name);
    if (name.isEmpty() || info.isAbsolute() || info.fileName() != name || name == "." || name == "..")
        return {};
    if (name.endsWith(' ') || name.endsWith('.')) return {};
    for (const QChar c : name) {
        if (c.isNull() || c.unicode() < 0x20 || QStringLiteral("<>:\"/\\\\|?*").contains(c))
            return {};
    }
    const QString stem = name.section('.', 0, 0).toUpper();
    static const QSet<QString> reserved{
        QStringLiteral("CON"), QStringLiteral("PRN"), QStringLiteral("AUX"), QStringLiteral("NUL"),
        QStringLiteral("COM1"), QStringLiteral("COM2"), QStringLiteral("COM3"), QStringLiteral("COM4"),
        QStringLiteral("COM5"), QStringLiteral("COM6"), QStringLiteral("COM7"), QStringLiteral("COM8"),
        QStringLiteral("COM9"), QStringLiteral("LPT1"), QStringLiteral("LPT2"), QStringLiteral("LPT3"),
        QStringLiteral("LPT4"), QStringLiteral("LPT5"), QStringLiteral("LPT6"), QStringLiteral("LPT7"),
        QStringLiteral("LPT8"), QStringLiteral("LPT9")};
    if (reserved.contains(stem)) return {};
    return name.normalized(QString::NormalizationForm_C).toCaseFolded();
}

inline qint64 entryLimit(const QString& key) {
    if (key == QStringLiteral("database")) return MaxDatabaseBytes;
    return portableIconKey(key).isEmpty() ? -1 : MaxIconBytes;
}

inline bool validateEntrySizes(const QMap<QString, qint64>& entries, qint64 metadataBytes,
                               QString* error) {
    if (entries.size() > MaxEntries)
        return fail(error, QStringLiteral("Archive contains too many entries"));

    qint64 encodedBytes = 4 + 4 + 4 + metadataBytes + 4;
    QSet<QString> portableIconNames;
    for (auto it = entries.constBegin(); it != entries.constEnd(); ++it) {
        const qint64 keyBytes = static_cast<qint64>(it.key().size()) * 2;
        const qint64 limit = entryLimit(it.key());
        if (keyBytes > MaxEntryNameBytes || limit < 0)
            return fail(error, QStringLiteral("Invalid archive entry: %1").arg(it.key()));
        if (it.key() != QStringLiteral("database")) {
            const QString portableName = portableIconKey(it.key());
            if (portableIconNames.contains(portableName))
                return fail(error, QStringLiteral("Conflicting archive entry: %1").arg(it.key()));
            portableIconNames.insert(portableName);
        }
        if (it.value() < 0 || it.value() > limit)
            return fail(error, QStringLiteral("Archive entry is too large: %1").arg(it.key()));
        encodedBytes += 4 + keyBytes + 4 + it.value();
        if (encodedBytes > MaxArchiveBytes)
            return fail(error, QStringLiteral("Archive exceeds 256 MiB"));
    }
    return true;
}

inline bool validateFiles(const QMap<QString, QByteArray>& files, qint64 metadataBytes,
                          QString* error) {
    QMap<QString, qint64> sizes;
    for (auto it = files.constBegin(); it != files.constEnd(); ++it)
        sizes.insert(it.key(), it.value().size());
    return validateEntrySizes(sizes, metadataBytes, error);
}

inline QString serializeMetadata(const QJsonObject& metadata) {
    return QString::fromUtf8(QJsonDocument(metadata).toJson(QJsonDocument::Compact));
}

inline qint64 metadataSize(const QJsonObject& metadata) {
    return static_cast<qint64>(serializeMetadata(metadata).size()) * 2;
}

inline bool write(QIODevice& device, const QJsonObject& metadata,
                  const QMap<QString, QByteArray>& files, QString* error) {
    const QString metadataJson = serializeMetadata(metadata);
    const qint64 metadataBytes = static_cast<qint64>(metadataJson.size()) * 2;
    if (metadataBytes > MaxMetadataBytes)
        return fail(error, QStringLiteral("Archive metadata is too large"));
    if (!validateFiles(files, metadataBytes, error)) return false;

    QDataStream stream(&device);
    stream.setByteOrder(QDataStream::LittleEndian);
    stream.setVersion(QDataStream::Qt_6_0);
    stream.writeRawData("THRN", 4);
    stream << FormatVersion << metadataJson << files;
    if (stream.status() != QDataStream::Ok)
        return fail(error, QStringLiteral("Failed to write archive"));
    return true;
}

inline bool read(QIODevice& device, Archive* archive, QString* error) {
    if (!archive || device.isSequential() || device.size() < 0 || device.size() > MaxArchiveBytes)
        return fail(error, QStringLiteral("Archive exceeds 256 MiB or is not seekable"));

    QDataStream stream(&device);
    stream.setByteOrder(QDataStream::LittleEndian);
    stream.setVersion(QDataStream::Qt_6_0);

    char magic[4] = {};
    if (stream.readRawData(magic, 4) != 4 || QByteArrayView(magic, 4) != QByteArrayView("THRN", 4))
        return fail(error, QStringLiteral("Invalid archive signature"));

    stream >> archive->formatVersion;
    if (stream.status() != QDataStream::Ok || archive->formatVersion < 1 || archive->formatVersion > FormatVersion)
        return fail(error, QStringLiteral("Unsupported archive format version"));

    QString metadataJson;
    if (!readString(stream, MaxMetadataBytes, &metadataJson, error)) return false;
    QJsonParseError parseError;
    const QJsonDocument metadataDocument = QJsonDocument::fromJson(metadataJson.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !metadataDocument.isObject())
        return fail(error, QStringLiteral("Invalid archive metadata"));
    archive->metadata = metadataDocument.object();

    quint32 entryCount = 0;
    stream >> entryCount;
    if (stream.status() != QDataStream::Ok || entryCount > MaxEntries)
        return fail(error, QStringLiteral("Archive contains too many entries"));

    archive->files.clear();
    qint64 totalPayload = 0;
    QSet<QString> portableIconNames;
    for (quint32 i = 0; i < entryCount; ++i) {
        QString key;
        if (!readString(stream, MaxEntryNameBytes, &key, error)) return false;
        const qint64 limit = entryLimit(key);
        if (limit < 0) return fail(error, QStringLiteral("Invalid archive entry: %1").arg(key));
        if (archive->files.contains(key))
            return fail(error, QStringLiteral("Duplicate archive entry: %1").arg(key));
        if (key != QStringLiteral("database")) {
            const QString portableName = portableIconKey(key);
            if (portableIconNames.contains(portableName))
                return fail(error, QStringLiteral("Conflicting archive entry: %1").arg(key));
            portableIconNames.insert(portableName);
        }

        QByteArray value;
        if (!readBytes(stream, limit, &value, error)) return false;
        if (value.size() > MaxArchiveBytes - totalPayload)
            return fail(error, QStringLiteral("Archive payload exceeds 256 MiB"));
        totalPayload += value.size();
        archive->files.insert(key, std::move(value));
    }

    if (stream.status() != QDataStream::Ok || device.bytesAvailable() != 0)
        return fail(error, QStringLiteral("Archive is corrupt or contains trailing data"));
    return true;
}

inline bool partsForRestore(const Archive& archive, Parts* parts, QString* error) {
    if (!parts) return fail(error, QStringLiteral("Missing backup parts output"));

    bool hasIcons = false;
    for (auto it = archive.files.constBegin(); it != archive.files.constEnd(); ++it) {
        if (it.key().startsWith(QStringLiteral("icons/"))) {
            hasIcons = true;
            break;
        }
    }

    Parts result;
    if (archive.formatVersion == 1) {
        result.profiles = result.routes = result.settings = archive.files.contains(QStringLiteral("database"));
        result.icons = hasIcons;
        *parts = result;
        return true;
    }

    const QJsonValue partsValue = archive.metadata.value(QStringLiteral("parts"));
    if (!partsValue.isObject())
        return fail(error, QStringLiteral("Backup metadata is missing its parts declaration"));

    const QJsonObject object = partsValue.toObject();
    for (const QString& key : {QStringLiteral("profiles"), QStringLiteral("routes"),
                               QStringLiteral("settings"), QStringLiteral("icons")}) {
        if (!object.value(key).isBool())
            return fail(error, QStringLiteral("Backup metadata has an invalid '%1' part").arg(key));
    }

    result.profiles = object.value(QStringLiteral("profiles")).toBool();
    result.routes = object.value(QStringLiteral("routes")).toBool();
    result.settings = object.value(QStringLiteral("settings")).toBool();
    result.icons = object.value(QStringLiteral("icons")).toBool();

    if (result.anyDatabase() != archive.files.contains(QStringLiteral("database")))
        return fail(error, QStringLiteral("Backup database does not match its parts declaration"));
    if (!result.icons && hasIcons)
        return fail(error, QStringLiteral("Backup icons do not match their parts declaration"));

    *parts = result;
    return true;
}

} // namespace BackupArchive
