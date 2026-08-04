#include "include/ui/setting/BackupRestore.h"

#include <QDir>
#include <QFile>
#include <QTemporaryDir>

#include <cstdlib>
#include <stdexcept>

namespace {

QByteArray readFile(const QString& path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) return {};
    return file.readAll();
}

bool writeFile(const QString& path, const QByteArray& bytes) {
    QString error;
    return BackupRestore::writeBytes(path, bytes, &error);
}

bool hasNoArtifacts(const QString& rootPath) {
    for (const char* name : {BackupRestore::JournalFileName,
                             BackupRestore::BeforeDatabaseFileName,
                             BackupRestore::StagedDatabaseFileName,
                             BackupRestore::BeforeIconsName,
                             BackupRestore::StagedIconsName}) {
        if (BackupRestore::exists(BackupRestore::path(rootPath, name))) return false;
    }
    return true;
}

BackupRestore::DatabaseActions fakeDatabase(QByteArray* liveDatabase,
                                            bool failApply = false,
                                            bool failRollback = false) {
    BackupRestore::DatabaseActions actions;
    actions.backup = [liveDatabase](const QString& path) {
        if (!writeFile(path, *liveDatabase))
            throw std::runtime_error("backup failed");
    };
    actions.apply = [liveDatabase, failApply](const QString& path) {
        *liveDatabase = readFile(path);
        if (failApply) throw std::runtime_error("apply failed");
    };
    actions.rollback = [liveDatabase, failRollback](const QString& path) {
        if (failRollback) throw std::runtime_error("rollback failed");
        *liveDatabase = readFile(path);
    };
    return actions;
}

BackupRestore::Request requestFor(const QString& rootPath) {
    BackupRestore::Request request;
    request.rootPath = rootPath;
    request.database = true;
    request.icons = true;
    request.files = {
        {QStringLiteral("database"), QByteArrayLiteral("new-database")},
        {QStringLiteral("icons/new.png"), QByteArrayLiteral("new-icon")},
    };
    return request;
}

bool commitsDatabaseAndIcons() {
    QTemporaryDir root;
    if (!root.isValid() || !QDir(root.path()).mkdir("icons") ||
        !writeFile(QDir(root.path()).filePath("icons/old.png"), "old-icon"))
        return false;

    QByteArray database = "old-database";
    const auto result = BackupRestore::execute(
        requestFor(root.path()), fakeDatabase(&database));
    return result.success && !result.recoveryPending && database == "new-database"
        && readFile(QDir(root.path()).filePath("icons/new.png")) == "new-icon"
        && !QFileInfo::exists(QDir(root.path()).filePath("icons/old.png"))
        && hasNoArtifacts(root.path());
}

bool rollsBackAfterApplyFailure() {
    QTemporaryDir root;
    if (!root.isValid() || !QDir(root.path()).mkdir("icons") ||
        !writeFile(QDir(root.path()).filePath("icons/old.png"), "old-icon"))
        return false;

    QByteArray database = "old-database";
    const auto result = BackupRestore::execute(
        requestFor(root.path()), fakeDatabase(&database, true));
    return !result.success && !result.recoveryPending && database == "old-database"
        && readFile(QDir(root.path()).filePath("icons/old.png")) == "old-icon"
        && !QFileInfo::exists(QDir(root.path()).filePath("icons/new.png"))
        && hasNoArtifacts(root.path());
}

bool leavesJournalWhenRollbackFails() {
    QTemporaryDir root;
    if (!root.isValid() || !QDir(root.path()).mkdir("icons") ||
        !writeFile(QDir(root.path()).filePath("icons/old.png"), "old-icon"))
        return false;

    QByteArray database = "old-database";
    const auto result = BackupRestore::execute(
        requestFor(root.path()), fakeDatabase(&database, true, true));
    return !result.success && result.recoveryPending
        && BackupRestore::exists(
            BackupRestore::path(root.path(), BackupRestore::JournalFileName));
}

bool recoversInterruptedRestoreBeforeDatabaseOpen() {
    QTemporaryDir root;
    if (!root.isValid() || !QDir(root.path()).mkdir("icons") ||
        !QDir(root.path()).mkdir(BackupRestore::BeforeIconsName))
        return false;

    const QString liveDatabase = QDir(root.path()).filePath("throne.db");
    if (!writeFile(liveDatabase, "new-database") ||
        !writeFile(BackupRestore::path(root.path(), BackupRestore::BeforeDatabaseFileName),
                   "old-database") ||
        !writeFile(QDir(root.path()).filePath("icons/new.png"), "new-icon") ||
        !writeFile(QDir(BackupRestore::path(root.path(), BackupRestore::BeforeIconsName))
                       .filePath("old.png"),
                   "old-icon"))
        return false;

    BackupRestore::Journal journal;
    journal.phase = QStringLiteral("icons_swapped");
    journal.database = true;
    journal.icons = true;
    journal.hadIcons = true;
    QString error;
    if (!BackupRestore::writeJournal(root.path(), journal, &error) ||
        !BackupRestore::recoverPending(root.path(), liveDatabase, &error))
        return false;

    return readFile(liveDatabase) == "old-database"
        && readFile(QDir(root.path()).filePath("icons/old.png")) == "old-icon"
        && !QFileInfo::exists(QDir(root.path()).filePath("icons/new.png"))
        && hasNoArtifacts(root.path());
}

bool cleanupFailureAfterRollbackIsNotRecoveryPending() {
    QTemporaryDir root;
    if (!root.isValid()) return false;

    BackupRestore::Request request;
    request.rootPath = root.path();
    request.database = true;
    request.files = {{QStringLiteral("database"), QByteArrayLiteral("new-database")}};

    QByteArray database = "old-database";
    BackupRestore::DatabaseActions actions;
    actions.backup = [&database](const QString& path) {
        if (!writeFile(path, database)) throw std::runtime_error("backup failed");
    };
    actions.apply = [&database](const QString& path) {
        database = readFile(path);
        throw std::runtime_error("apply failed");
    };
    actions.rollback = [&database, rootPath = root.path()](const QString& path) {
        database = readFile(path);
        QFile(rootPath).setPermissions(QFileDevice::ReadOwner | QFileDevice::ExeOwner);
    };

    const auto result = BackupRestore::execute(request, actions);
    QFile(root.path()).setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                                      QFileDevice::ExeOwner);
    return !result.success && !result.recoveryPending && database == "old-database"
        && result.error.contains(QStringLiteral("Cleanup:"))
        && BackupRestore::recoverPending(root.path(), QDir(root.path()).filePath("throne.db"), nullptr);
}

} // namespace

int main() {
    return commitsDatabaseAndIcons() && rollsBackAfterApplyFailure()
        && leavesJournalWhenRollbackFails()
        && recoversInterruptedRestoreBeforeDatabaseOpen()
        && cleanupFailureAfterRollbackIsNotRecoveryPending()
        ? EXIT_SUCCESS : EXIT_FAILURE;
}
