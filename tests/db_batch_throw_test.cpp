#include "include/database/Database.h"

#include <3rdparty/SQLiteCpp/include/SQLiteCpp.h>

#include <QTemporaryDir>

#include <cstdlib>
#include <functional>
#include <string>
#include <vector>

int MessageBoxWarning(const QString&, const QString&) {
    return 0;
}

void runOnUiThread(const std::function<void()>& callback, bool) {
    callback();
}

namespace {

bool insertsThenRollsBackOnFailure(const QString& path) {
    Configs::Database db(path.toStdString());
    db.execThrow(R"(
        CREATE TABLE profiles (
            id INTEGER PRIMARY KEY,
            type TEXT NOT NULL,
            name TEXT,
            gid INTEGER NOT NULL DEFAULT 0,
            latency INTEGER NOT NULL DEFAULT 0,
            latency_at INTEGER NOT NULL DEFAULT 0,
            dl_speed TEXT,
            ul_speed TEXT,
            test_country TEXT,
            ip_out TEXT,
            outbound_json TEXT NOT NULL,
            traffic_dl INTEGER NOT NULL DEFAULT 0,
            traffic_up INTEGER NOT NULL DEFAULT 0
        )
    )");

    Configs::ProfileInsertRow row{
        .id = 1, .type = "socks", .name = "a", .outbound_json = "{}"
    };
    try {
        auto connLock = db.lockConnection();
        db.execThrow("BEGIN IMMEDIATE");
        db.execBatchInsertProfilesThrow({row});
        // Force failure after a successful insert inside the same transaction.
        db.execThrow("INSERT INTO profiles (id, type, name, gid, latency, dl_speed, ul_speed, test_country, ip_out, outbound_json) VALUES (1,'x','y',0,0,'','','','','{}')");
        db.execThrow("COMMIT");
        return false;
    } catch (const std::exception&) {
        try { db.execThrow("ROLLBACK"); } catch (...) {}
    }

    auto q = db.query("SELECT COUNT(*) FROM profiles");
    if (!q || !q->executeStep()) return false;
    return q->getColumn(0).getInt() == 0;
}

bool deleteThrowRemovesRows(const QString& path) {
    Configs::Database db(path.toStdString());
    db.execThrow(R"(
        CREATE TABLE profiles (
            id INTEGER PRIMARY KEY,
            type TEXT NOT NULL,
            name TEXT,
            gid INTEGER NOT NULL DEFAULT 0,
            latency INTEGER NOT NULL DEFAULT 0,
            latency_at INTEGER NOT NULL DEFAULT 0,
            dl_speed TEXT,
            ul_speed TEXT,
            test_country TEXT,
            ip_out TEXT,
            outbound_json TEXT NOT NULL,
            traffic_dl INTEGER NOT NULL DEFAULT 0,
            traffic_up INTEGER NOT NULL DEFAULT 0
        )
    )");
    Configs::ProfileInsertRow a{.id = 1, .type = "socks", .name = "a", .outbound_json = "{}"};
    Configs::ProfileInsertRow b{.id = 2, .type = "socks", .name = "b", .outbound_json = "{}"};
    {
        auto connLock = db.lockConnection();
        db.execThrow("BEGIN IMMEDIATE");
        db.execBatchInsertProfilesThrow({a, b});
        db.execDeleteByIdInThrow("profiles", "id", {1});
        db.execThrow("COMMIT");
    }
    auto q = db.query("SELECT id FROM profiles ORDER BY id");
    if (!q || !q->executeStep()) return false;
    if (q->getColumn(0).getInt() != 2) return false;
    return !q->executeStep();
}

} // namespace

namespace {

// A statement whose underlying schema changes mid-flight makes sqlite3_step
// throw SQLite::Exception. LockedStatement::tryStep must swallow that and
// report failure instead of letting the exception escape (std::terminate in a
// Qt slot on the real read paths).
bool tryStepSwallowsStepFailure(const QString& path) {
    Configs::Database db(path.toStdString());
    db.exec("CREATE TABLE t (id INTEGER)");
    db.exec("INSERT INTO t VALUES (1)");
    auto q = db.query("SELECT id FROM t");
    if (!q) return false;
    {
        // Drop the table behind the prepared statement's back via a second
        // connection (the repo lock only serializes this Database instance).
        SQLite::Database other(path.toStdString(), SQLite::OPEN_READWRITE);
        other.exec("DROP TABLE t");
    }
    try {
        (void) q.tryStep();
    } catch (...) {
        return false;
    }
    return true;
}

} // namespace

int main() {
    QTemporaryDir dir;
    if (!dir.isValid()) return EXIT_FAILURE;
    const QString path1 = dir.filePath("roll.db");
    const QString path2 = dir.filePath("del.db");
    const QString path3 = dir.filePath("step.db");
    if (!insertsThenRollsBackOnFailure(path1)) return EXIT_FAILURE;
    if (!deleteThrowRemovesRows(path2)) return EXIT_FAILURE;
    if (!tryStepSwallowsStepFailure(path3)) return EXIT_FAILURE;
    return EXIT_SUCCESS;
}
