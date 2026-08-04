#pragma once

#include "include/ui/setting/BackupArchive.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMap>
#include <QSaveFile>
#include <QString>

#include <functional>

namespace BackupRestore {

inline constexpr auto JournalFileName = ".throne-restore-journal.json";
inline constexpr auto BeforeDatabaseFileName = ".throne-restore-before.db";
inline constexpr auto StagedDatabaseFileName = ".throne-restore-new.db";
inline constexpr auto BeforeIconsName = ".throne-restore-icons-before";
inline constexpr auto StagedIconsName = ".throne-restore-icons-new";
inline constexpr auto LiveIconsName = "icons";

struct Request {
    QString rootPath;
    bool database = false;
    bool icons = false;
    QMap<QString, QByteArray> files;
};

struct DatabaseActions {
    std::function<void(const QString&)> backup;
    std::function<void(const QString&)> apply;
    std::function<void(const QString&)> rollback;
};

struct Result {
    bool success = false;
    bool recoveryPending = false;
    QString error;
};

struct Journal {
    QString phase;
    bool database = false;
    bool icons = false;
    bool hadIcons = false;
};

inline QString path(const QString& rootPath, const char* name) {
    return QDir(rootPath).filePath(QString::fromLatin1(name));
}

inline bool exists(const QString& filePath) {
    const QFileInfo info(filePath);
    return info.exists() || info.isSymLink();
}

inline bool fail(QString* error, const QString& message) {
    if (error) *error = message;
    return false;
}

inline bool removePath(const QString& filePath, QString* error) {
    const QFileInfo info(filePath);
    if (!info.exists() && !info.isSymLink()) return true;

    const bool removed = info.isDir() && !info.isSymLink()
        ? QDir(filePath).removeRecursively()
        : QFile::remove(filePath);
    return removed || fail(error, QStringLiteral("Failed to remove restore artifact: %1").arg(filePath));
}

inline bool writeBytes(const QString& filePath, const QByteArray& bytes, QString* error) {
    QSaveFile file(filePath);
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size() || !file.commit())
        return fail(error, QStringLiteral("Failed to write restore artifact: %1").arg(filePath));
    return true;
}

inline bool copyAtomically(const QString& sourcePath, const QString& destinationPath, QString* error) {
    QFile source(sourcePath);
    QSaveFile destination(destinationPath);
    if (!source.open(QIODevice::ReadOnly) || !destination.open(QIODevice::WriteOnly))
        return fail(error, QStringLiteral("Failed to open a database recovery file"));

    QByteArray buffer(64 * 1024, Qt::Uninitialized);
    while (true) {
        const qint64 bytesRead = source.read(buffer.data(), buffer.size());
        if (bytesRead < 0)
            return fail(error, QStringLiteral("Failed to read the database recovery snapshot"));
        if (bytesRead == 0) break;
        if (destination.write(buffer.constData(), bytesRead) != bytesRead)
            return fail(error, QStringLiteral("Failed to write the recovered database"));
    }
    if (!destination.commit())
        return fail(error, QStringLiteral("Failed to commit the recovered database"));
    return true;
}

inline bool writeJournal(const QString& rootPath, const Journal& journal, QString* error) {
    const QJsonObject object{
        {QStringLiteral("version"), 1},
        {QStringLiteral("phase"), journal.phase},
        {QStringLiteral("database"), journal.database},
        {QStringLiteral("icons"), journal.icons},
        {QStringLiteral("had_icons"), journal.hadIcons},
    };
    return writeBytes(path(rootPath, JournalFileName),
                      QJsonDocument(object).toJson(QJsonDocument::Compact), error);
}

inline bool readJournal(const QString& rootPath, Journal* journal, QString* error) {
    QFile file(path(rootPath, JournalFileName));
    if (!file.open(QIODevice::ReadOnly))
        return fail(error, QStringLiteral("Failed to open the restore recovery journal"));

    QJsonParseError parseError;
    const QJsonDocument document = QJsonDocument::fromJson(file.readAll(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !document.isObject())
        return fail(error, QStringLiteral("The restore recovery journal is corrupt"));

    const QJsonObject object = document.object();
    const QString phase = object.value(QStringLiteral("phase")).toString();
    const bool validPhase = phase == QStringLiteral("prepared")
        || phase == QStringLiteral("icons_backed_up")
        || phase == QStringLiteral("icons_swapped")
        || phase == QStringLiteral("committed");
    if (object.value(QStringLiteral("version")).toInt() != 1 || !validPhase
        || !object.value(QStringLiteral("database")).isBool()
        || !object.value(QStringLiteral("icons")).isBool()
        || !object.value(QStringLiteral("had_icons")).isBool()) {
        return fail(error, QStringLiteral("The restore recovery journal has invalid fields"));
    }

    journal->phase = phase;
    journal->database = object.value(QStringLiteral("database")).toBool();
    journal->icons = object.value(QStringLiteral("icons")).toBool();
    journal->hadIcons = object.value(QStringLiteral("had_icons")).toBool();
    return true;
}

inline bool cleanupArtifacts(const QString& rootPath, bool removeJournal, QString* error) {
    bool ok = true;
    QString firstError;
    for (const char* name : {BeforeDatabaseFileName, StagedDatabaseFileName,
                             BeforeIconsName, StagedIconsName}) {
        QString currentError;
        if (!removePath(path(rootPath, name), &currentError)) {
            ok = false;
            if (firstError.isEmpty()) firstError = currentError;
        }
    }
    if (removeJournal) {
        QString currentError;
        if (!removePath(path(rootPath, JournalFileName), &currentError)) {
            ok = false;
            if (firstError.isEmpty()) firstError = currentError;
        }
    }
    if (!ok && error) *error = firstError;
    return ok;
}

inline bool rollbackIcons(const QString& rootPath, bool hadIcons, QString* error) {
    QDir root(rootPath);
    const QString livePath = path(rootPath, LiveIconsName);
    const QString backupPath = path(rootPath, BeforeIconsName);
    if (exists(backupPath)) {
        if (!removePath(livePath, error)) return false;
        if (!root.rename(QString::fromLatin1(BeforeIconsName), QString::fromLatin1(LiveIconsName)))
            return fail(error, QStringLiteral("Failed to restore the previous custom icons"));
        return true;
    }
    if (!hadIcons) return removePath(livePath, error);
    if (!exists(livePath))
        return fail(error, QStringLiteral("The previous custom icon directory is missing"));
    return true;
}

inline bool recoverPending(const QString& rootPath, const QString& liveDatabasePath, QString* error) {
    const QString journalPath = path(rootPath, JournalFileName);
    if (!exists(journalPath)) {
        QString cleanupError;
        cleanupArtifacts(rootPath, false, &cleanupError);
        return true;
    }

    Journal journal;
    if (!readJournal(rootPath, &journal, error)) return false;
    if (journal.phase == QStringLiteral("committed"))
        return cleanupArtifacts(rootPath, true, error);

    if (journal.database) {
        const QString snapshotPath = path(rootPath, BeforeDatabaseFileName);
        if (!exists(snapshotPath))
            return fail(error, QStringLiteral("The database recovery snapshot is missing"));
        if (!removePath(liveDatabasePath + QStringLiteral("-wal"), error)
            || !removePath(liveDatabasePath + QStringLiteral("-shm"), error)
            || !removePath(liveDatabasePath + QStringLiteral("-journal"), error)
            || !copyAtomically(snapshotPath, liveDatabasePath, error)) {
            return false;
        }
    }
    if (journal.icons && !rollbackIcons(rootPath, journal.hadIcons, error)) return false;
    return cleanupArtifacts(rootPath, true, error);
}

inline bool stageIcons(const Request& request, QString* error) {
    QDir root(request.rootPath);
    if (!root.mkdir(QString::fromLatin1(StagedIconsName)))
        return fail(error, QStringLiteral("Failed to create the custom icon staging directory"));

    const QString stagingPath = path(request.rootPath, StagedIconsName);
    for (auto it = request.files.constBegin(); it != request.files.constEnd(); ++it) {
        if (!it.key().startsWith(QStringLiteral("icons/"))) continue;
        if (BackupArchive::portableIconKey(it.key()).isEmpty())
            return fail(error, QStringLiteral("Invalid custom icon entry: %1").arg(it.key()));
        if (!writeBytes(QDir(stagingPath).filePath(it.key().mid(6)), it.value(), error)) return false;
    }
    return true;
}

inline bool swapIcons(const QString& rootPath, Journal* journal, QString* error) {
    QDir root(rootPath);
    if (journal->hadIcons) {
        if (!root.rename(QString::fromLatin1(LiveIconsName), QString::fromLatin1(BeforeIconsName)))
            return fail(error, QStringLiteral("Failed to preserve the current custom icons"));
        journal->phase = QStringLiteral("icons_backed_up");
        if (!writeJournal(rootPath, *journal, error)) return false;
    }

    if (!root.rename(QString::fromLatin1(StagedIconsName), QString::fromLatin1(LiveIconsName)))
        return fail(error, QStringLiteral("Failed to install the restored custom icons"));
    journal->phase = QStringLiteral("icons_swapped");
    return writeJournal(rootPath, *journal, error);
}

inline Result execute(const Request& request, const DatabaseActions& database) {
    Result result;
    if (!request.database && !request.icons) {
        result.error = QStringLiteral("No backup part was selected for restore");
        return result;
    }
    if (request.database && (!database.backup || !database.apply || !database.rollback)) {
        result.error = QStringLiteral("Database restore actions are incomplete");
        return result;
    }
    if (!QDir(request.rootPath).exists()) {
        result.error = QStringLiteral("The restore directory does not exist");
        return result;
    }
    if (exists(path(request.rootPath, JournalFileName))) {
        result.recoveryPending = true;
        result.error = QStringLiteral("A previous restore is waiting for startup recovery");
        return result;
    }

    QString operationError;
    if (!cleanupArtifacts(request.rootPath, false, &operationError)) {
        result.error = operationError;
        return result;
    }

    bool journalWritten = false;
    Journal journal;
    journal.phase = QStringLiteral("prepared");
    journal.database = request.database;
    journal.icons = request.icons;
    journal.hadIcons = request.icons && exists(path(request.rootPath, LiveIconsName));

    auto rollback = [&] {
        bool recovered = true;
        QString errors;
        if (journal.database) {
            try {
                database.rollback(path(request.rootPath, BeforeDatabaseFileName));
            } catch (const std::exception& exception) {
                recovered = false;
                errors = QString::fromUtf8(exception.what());
            }
        }
        if (journal.icons) {
            QString iconError;
            if (!rollbackIcons(request.rootPath, journal.hadIcons, &iconError)) {
                recovered = false;
                if (!errors.isEmpty()) errors += QStringLiteral("; ");
                errors += iconError;
            }
        }
        QString cleanupError;
        if (recovered && !cleanupArtifacts(request.rootPath, true, &cleanupError)) {
            if (!operationError.isEmpty()) operationError += QStringLiteral(" ");
            operationError += QStringLiteral("Cleanup: ") + cleanupError;
        }
        if (!recovered && !errors.isEmpty()) operationError += QStringLiteral(" Recovery: ") + errors;
        return recovered;
    };

    try {
        if (request.database) {
            const auto databaseEntry = request.files.constFind(QStringLiteral("database"));
            if (databaseEntry == request.files.constEnd())
                throw QStringLiteral("The selected backup has no database entry");
            if (!writeBytes(path(request.rootPath, StagedDatabaseFileName), databaseEntry.value(), &operationError))
                throw operationError;
        }
        if (request.icons && !stageIcons(request, &operationError)) throw operationError;
        if (request.database)
            database.backup(path(request.rootPath, BeforeDatabaseFileName));

        if (!writeJournal(request.rootPath, journal, &operationError)) throw operationError;
        journalWritten = true;

        if (request.icons && !swapIcons(request.rootPath, &journal, &operationError)) throw operationError;
        if (request.database)
            database.apply(path(request.rootPath, StagedDatabaseFileName));

        journal.phase = QStringLiteral("committed");
        if (!writeJournal(request.rootPath, journal, &operationError)) throw operationError;

        QString cleanupError;
        cleanupArtifacts(request.rootPath, true, &cleanupError);
        result.success = true;
        return result;
    } catch (const QString& message) {
        operationError = message;
    } catch (const std::exception& exception) {
        operationError = QString::fromUtf8(exception.what());
    }

    if (!journalWritten) {
        QString cleanupError;
        cleanupArtifacts(request.rootPath, true, &cleanupError);
        result.error = operationError;
        return result;
    }

    result.recoveryPending = !rollback();
    result.error = operationError;
    return result;
}

} // namespace BackupRestore
