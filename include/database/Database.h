#pragma once

#include <3rdparty/SQLiteCpp/include/SQLiteCpp.h>
#include <atomic>
#include <string>
#include <iostream>
#include <vector>
#include <utility>
#include <type_traits>
#include <mutex>
#include <memory>

#include "include/global/Utils.hpp"

namespace Configs {
    struct ProfileInsertRow {
        int id;
        std::string type;
        std::string name;
        int gid;
        int latency;
        long long latency_at = 0;
        int mux_capability = 0;
        long long mux_capability_at = 0;
        std::string dl_speed;
        std::string ul_speed;
        std::string test_country;
        std::string ip_out;
        std::string outbound_json;
        long long traffic_dl = 0;
        long long traffic_up = 0;
    };
    // Which logical categories a backup contains / a restore should apply.
    // profiles -> groups, groups_order, profiles tables
    // routes   -> route_profiles, route_rules tables
    // settings -> settings table
    // icons    -> icons/ folder (handled by the UI layer, not the database)
    struct BackupParts {
        bool profiles = false;
        bool routes = false;
        bool settings = false;
        bool icons = false;

        [[nodiscard]] bool anyDb() const { return profiles || routes || settings; }
        [[nodiscard]] bool any() const { return profiles || routes || settings || icons; }
    };

    // Max bound parameters per statement (SQLite default SQLITE_MAX_VARIABLE_NUMBER is 999).
    constexpr int BATCH_LIMIT_WRITE = 1500;
    constexpr int BATCH_LIMIT_READ = 4096;
    // Run WAL checkpoint after this many write operations (exec or batch chunk).
    constexpr int WAL_CHECKPOINT_AFTER_WRITES = 10000;

    // Reclaim free-list pages with a one-time VACUUM at startup when the file has
    // accumulated significant dead space (SQLite never returns freed pages to the
    // OS on its own without auto_vacuum). Both thresholds must be met, so we never
    // VACUUM away trivial fragmentation on every launch:
    //   - at least VACUUM_MIN_FREE_BYTES of free-list pages (absolute floor), and
    //   - free pages make up at least VACUUM_MIN_FREE_RATIO of the whole file.
    constexpr long long VACUUM_MIN_FREE_BYTES = 4LL * 1024 * 1024; // 4 MiB
    constexpr double VACUUM_MIN_FREE_RATIO = 0.50;                 // 50%

    inline void NotifyError(const std::string& query, std::exception& e) {
        runOnUiThread([=] {
            std::string shortQ;
            if (query.length() > 200) shortQ = query.substr(0, 200);
            else shortQ = query;
            MessageBoxWarning("DB error occurred", "Failed for " + QString::fromStdString(shortQ) + " with exception: " + e.what() + "\n your database may be corrupted");
        });
    }

    // Holds the connection lock for the full lifetime of a Statement so
    // executeStep/getColumn stay serialized on the shared sqlite3*.
    class LockedStatement {
        std::unique_lock<std::recursive_mutex> lock_;
        std::unique_ptr<SQLite::Statement> stmt_;
    public:
        LockedStatement() = default;
        LockedStatement(std::unique_lock<std::recursive_mutex> lock,
                        std::unique_ptr<SQLite::Statement> stmt)
            : lock_(std::move(lock)), stmt_(std::move(stmt)) {}
        SQLite::Statement* get() const { return stmt_.get(); }
        SQLite::Statement* operator->() const { return stmt_.get(); }
        SQLite::Statement& operator*() const { return *stmt_; }
        explicit operator bool() const { return static_cast<bool>(stmt_); }

        // Non-throwing executeStep for read iteration. A corrupted database
        // makes sqlite3_step throw SQLite::Exception, and an exception escaping
        // a Qt slot is std::terminate, so read paths iterate with tryStep and
        // take their existing failure return (empty list / nullptr / false).
        // Once executeStep() succeeded, getColumn() on the prepared column set
        // does not throw, so guarding the step covers the corruption path.
        bool tryStep() noexcept {
            try {
                return stmt_ && stmt_->executeStep();
            } catch (const std::exception& e) {
                qWarning() << "DB read failed:" << e.what();
                return false;
            }
        }
    };

    class Database {
        SQLite::Database db;
        mutable std::recursive_mutex mu;
        std::atomic<int> writeCount{0};
        void maybeCheckpoint(int count);
        void maybeVacuum();

        void execDeleteByIdInChunk(const std::string& table, const std::string& idColumn, const std::vector<int>& ids);
        void execBatchSettingsReplaceChunk(const std::vector<std::pair<std::string, std::string>>& keyValues);
        void execBatchInsertIntPairsChunk(const std::string& table, const std::string& colA, const std::string& colB,
                                         const std::vector<int>& pairs);
        void execBatchInsertProfilesChunk(const std::vector<ProfileInsertRow>& rows);
        void execBatchReplaceProfilesChunk(const std::vector<ProfileInsertRow>& rows);
    public:
        // Hold across multi-statement BEGIN/COMMIT so other threads cannot
        // interleave on the same connection between statements.
        std::unique_lock<std::recursive_mutex> lockConnection() const {
            return std::unique_lock<std::recursive_mutex>(mu);
        }

        Database(const std::string& path)
            : db(path, SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE) {
            db.setBusyTimeout(5000);
            db.exec("PRAGMA foreign_keys = ON");
            db.exec("PRAGMA journal_mode = WAL");
            db.exec("PRAGMA synchronous = NORMAL");
            db.exec("PRAGMA mmap_size = 67108864"); // 64MB
            checkpointWal();
            maybeVacuum();
        }

    private:

        // 1. Bind one argument; explicit overloads avoid ambiguity on Linux (int32_t/int64_t/uint32_t)
        template<typename T>
        std::enable_if_t<std::is_integral_v<std::decay_t<T>>> bindOne(SQLite::Statement& query, int index, T&& value) {
            query.bind(index, static_cast<int64_t>(value));
        }
        template<typename T>
        std::enable_if_t<std::is_floating_point_v<std::decay_t<T>>> bindOne(SQLite::Statement& query, int index, T&& value) {
            query.bind(index, static_cast<double>(value));
        }
        void bindOne(SQLite::Statement& query, int index, const std::string& value) {
            query.bind(index, value);
        }
        void bindOne(SQLite::Statement& query, int index, const char* value) {
            query.bind(index, value);
        }

        template<typename T, typename... Rest>
        void bindArgs(SQLite::Statement& query, int index, T&& first, Rest&&... rest) {
            bindOne(query, index, std::forward<T>(first));
            bindArgs(query, index + 1, std::forward<Rest>(rest)...);
        }

        void bindArgs(SQLite::Statement& query, int index) {
            // No more args to bind
        }

        // 2. The "PGX Style" Exec (No return value, e.g., UPDATE/INSERT)
        template<typename... Args>
        void exec0(const std::string& sql, Args&&... args) {
            SQLite::Statement query(db, sql);
            bindArgs(query, 1, std::forward<Args>(args)...);
            query.exec();
            maybeCheckpoint(1);
        }

        // Run WAL checkpoint manually (e.g. from a periodic timer). Safe to call from any thread.
        void checkpointWal();

        // 3. Helper for fetching a single row
        // Returns a Statement you can extract data from
        template<typename... Args>
        std::unique_ptr<SQLite::Statement> query0(const std::string& sql, Args&&... args) {
            auto query = std::make_unique<SQLite::Statement>(db, sql);
            bindArgs(*query, 1, std::forward<Args>(args)...);
            return query;
        }

        // 4. Execute DELETE FROM table WHERE idColumn IN (ids), chunked by BATCH_LIMIT
        void execDeleteByIdIn0(const std::string& table, const std::string& idColumn, const std::vector<int>& ids) {
            for (size_t off = 0; off < ids.size(); off += BATCH_LIMIT_WRITE) {
                size_t end = std::min(off + BATCH_LIMIT_WRITE, ids.size());
                std::vector<int> chunk(ids.begin() + static_cast<std::ptrdiff_t>(off), ids.begin() + static_cast<std::ptrdiff_t>(end));
                execDeleteByIdInChunk(table, idColumn, chunk);
            }
        }

        // 5. Execute INSERT OR REPLACE INTO settings (key, value) VALUES ..., chunked (2 params per row -> BATCH_LIMIT/2 rows per chunk)
        void execBatchSettingsReplace0(const std::vector<std::pair<std::string, std::string>>& keyValues) {
            const size_t chunkSize = BATCH_LIMIT_WRITE / 2;
            for (size_t off = 0; off < keyValues.size(); off += chunkSize) {
                size_t end = std::min(off + chunkSize, keyValues.size());
                std::vector<std::pair<std::string, std::string>> chunk(keyValues.begin() + static_cast<std::ptrdiff_t>(off),
                                                                       keyValues.begin() + static_cast<std::ptrdiff_t>(end));
                execBatchSettingsReplaceChunk(chunk);
            }
        }

        // 6. Execute INSERT INTO table (colA, colB) VALUES ..., chunked (2 params per pair -> BATCH_LIMIT/2 pairs per chunk)
        void execBatchInsertIntPairs0(const std::string& table, const std::string& colA, const std::string& colB,
                                     const std::vector<int>& pairs) {
            if (pairs.size() < 2 || pairs.size() % 2 != 0) return;
            const size_t chunkPairs = BATCH_LIMIT_WRITE / 2;
            for (size_t off = 0; off < pairs.size() / 2; off += chunkPairs) {
                size_t pairCount = std::min(chunkPairs, pairs.size() / 2 - off);
                std::vector<int> chunk;
                chunk.reserve(pairCount * 2);
                for (size_t i = 0; i < pairCount; ++i) {
                    size_t idx = (off + i) * 2;
                    chunk.push_back(pairs[idx]);
                    chunk.push_back(pairs[idx + 1]);
                }
                execBatchInsertIntPairsChunk(table, colA, colB, chunk);
            }
        }

        // Chunked (13 params per row -> BATCH_LIMIT/13 rows per chunk)
        void execBatchInsertProfiles0(const std::vector<ProfileInsertRow>& rows) {
            const size_t chunkSize = BATCH_LIMIT_WRITE / 13;
            for (size_t off = 0; off < rows.size(); off += chunkSize) {
                size_t end = std::min(off + chunkSize, rows.size());
                std::vector<ProfileInsertRow> chunk(rows.begin() + static_cast<std::ptrdiff_t>(off),
                                                    rows.begin() + static_cast<std::ptrdiff_t>(end));
                execBatchInsertProfilesChunk(chunk);
            }
        }

        // Same chunking as execBatchInsertProfiles; INSERT OR REPLACE for batch save/update
        void execBatchReplaceProfiles0(const std::vector<ProfileInsertRow>& rows) {
            const size_t chunkSize = BATCH_LIMIT_WRITE / 13;
            for (size_t off = 0; off < rows.size(); off += chunkSize) {
                size_t end = std::min(off + chunkSize, rows.size());
                std::vector<ProfileInsertRow> chunk(rows.begin() + static_cast<std::ptrdiff_t>(off),
                                                    rows.begin() + static_cast<std::ptrdiff_t>(end));
                execBatchReplaceProfilesChunk(chunk);
            }
        }

    public:
        template<typename... Args>
        void exec(const std::string& sql, Args&&... args) {
            try {
                std::lock_guard<std::recursive_mutex> lock(mu);
                exec0(sql, std::forward<Args>(args)...);
            } catch (std::exception& e) {
                NotifyError(sql, e);
            }
        }

        // Throwing variant of exec(): lets callers compose an explicit
        // transaction (BEGIN/COMMIT/ROLLBACK) in which a failed statement must
        // abort the whole unit instead of being swallowed per-statement.
        // Multi-statement callers must hold lockConnection() across BEGIN..COMMIT.
        template<typename... Args>
        void execThrow(const std::string& sql, Args&&... args) {
            std::lock_guard<std::recursive_mutex> lock(mu);
            exec0(sql, std::forward<Args>(args)...);
        }

        template<typename... Args>
        LockedStatement query(const std::string& sql, Args&&... args) {
            try {
                std::unique_lock<std::recursive_mutex> lock(mu);
                auto q = query0(sql, std::forward<Args>(args)...);
                return LockedStatement(std::move(lock), std::move(q));
            } catch (std::exception& e) {
                NotifyError(sql, e);
                return LockedStatement{};
            }
        }

        void execDeleteByIdIn(const std::string& table, const std::string& idColumn, const std::vector<int>& ids) {
            try {
                std::lock_guard<std::recursive_mutex> lock(mu);
                execDeleteByIdIn0(table, idColumn, ids);
            } catch (std::exception& e) {
                NotifyError("execDeleteByIdIn for " + table, e);
            }
        }

        bool execBatchSettingsReplace(const std::vector<std::pair<std::string, std::string>>& keyValues) {
            try {
                std::lock_guard<std::recursive_mutex> lock(mu);
                execBatchSettingsReplace0(keyValues);
                return true;
            } catch (std::exception& e) {
                NotifyError("execBatchSettingsReplace", e);
                return false;
            }
        }

        void execBatchInsertIntPairs(const std::string& table, const std::string& colA, const std::string& colB,
                                     const std::vector<int>& pairs) {
            try {
                std::lock_guard<std::recursive_mutex> lock(mu);
                execBatchInsertIntPairs0(table, colA, colB, pairs);
            } catch (std::exception& e) {
                NotifyError("execBatchInsertIntPairs for " + table, e);
            }
        }

        // Throwing variant for multi-statement transactions (BEGIN/COMMIT/ROLLBACK).
        void execBatchInsertIntPairsThrow(const std::string& table, const std::string& colA, const std::string& colB,
                                          const std::vector<int>& pairs) {
            std::lock_guard<std::recursive_mutex> lock(mu);
            execBatchInsertIntPairs0(table, colA, colB, pairs);
        }

        void execBatchInsertProfiles(const std::vector<ProfileInsertRow>& rows) {
            try {
                std::lock_guard<std::recursive_mutex> lock(mu);
                execBatchInsertProfiles0(rows);
            } catch (std::exception& e) {
                NotifyError("execBatchInsertProfiles", e);
            }
        }

        // Throwing variants for multi-statement transactions (BEGIN/COMMIT/ROLLBACK).
        void execBatchInsertProfilesThrow(const std::vector<ProfileInsertRow>& rows) {
            std::lock_guard<std::recursive_mutex> lock(mu);
            execBatchInsertProfiles0(rows);
        }

        void execDeleteByIdInThrow(const std::string& table, const std::string& idColumn, const std::vector<int>& ids) {
            std::lock_guard<std::recursive_mutex> lock(mu);
            execDeleteByIdIn0(table, idColumn, ids);
        }

        void execBatchReplaceProfiles(const std::vector<ProfileInsertRow>& rows) {
            try {
                std::lock_guard<std::recursive_mutex> lock(mu);
                execBatchReplaceProfiles0(rows);
            } catch (std::exception& e) {
                NotifyError("execBatchReplaceProfiles", e);
            }
        }

        // Throws std::exception on failure. Caller must delete destPath if it already exists.
        void backupTo(const std::string& destPath);
        // Throws std::exception on failure.
        void restoreFrom(const std::string& srcPath);

        // Create a snapshot at destPath containing only the selected categories.
        // entity_ids is always retained so restored IDs stay consistent.
        // Caller must delete destPath if it already exists. Throws on failure.
        void backupSelective(const std::string& destPath, const BackupParts& parts);

        // Replace the selected categories in the live database with the contents
        // of the snapshot at srcPath. Only columns present in both schemas are
        // copied, so backups from other versions still restore. Throws on failure.
        void restoreSelective(const std::string& srcPath, const BackupParts& parts);
    };
}
