#include "include/database/SettingsRepo.h"

#include <3rdparty/SQLiteCpp/include/SQLiteCpp.h>

#include <QDebug>
#include <QKeySequence>
#include <QTemporaryDir>

#include <cstdlib>
#include <functional>
#include <stdexcept>

namespace {
    int warningCount = 0;

    bool roundTripsSettings(const QString& path) {
        Configs::Database db(path.toStdString());
        Configs::SettingsRepo saved(db);
        saved.disable_tray = true;
        saved.test_concurrent = 17;
        saved.log_level = "debug";
        saved.log_include_keyword = {"alpha", "beta"};
        saved.shortcuts["toggle"] = QKeySequence("Ctrl+T");
        saved.xray_vless_preference = Configs::Xray::AllVLESS;
        saved.sub_auto_update_last = 1234567890123;
        saved.session_system_proxy = true;
        saved.session_vpn = true;
        if (!saved.Save()) return false;

        Configs::SettingsRepo loaded(db);
        return loaded.disable_tray
            && loaded.test_concurrent == 17
            && loaded.log_level == "debug"
            && loaded.log_include_keyword == QStringList{"alpha", "beta"}
            && loaded.shortcuts.value("toggle") == QKeySequence("Ctrl+T")
            && loaded.xray_vless_preference == Configs::Xray::AllVLESS
            && loaded.sub_auto_update_last == 1234567890123
            && loaded.session_system_proxy
            && loaded.session_vpn;
    }

    bool reportsWriteFailure(const QString& path) {
        Configs::Database db(path.toStdString());
        Configs::SettingsRepo settings(db);
        db.exec("DROP TABLE settings");
        const int previousWarnings = warningCount;
        return !settings.Save() && warningCount == previousWarnings + 1;
    }

    bool skipsWriteWhenDisabled(const QString& path) {
        Configs::Database db(path.toStdString());
        Configs::SettingsRepo settings(db);
        db.exec("DROP TABLE settings");
        settings.noSave = true;
        const int previousWarnings = warningCount;
        return settings.Save() && warningCount == previousWarnings;
    }

    bool rejectsFailedQuery(const QString& path) {
        {
            SQLite::Database raw(path.toStdString(), SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE);
            raw.exec("CREATE TABLE settings (other TEXT)");
        }
        try {
            Configs::Database db(path.toStdString());
            Configs::SettingsRepo settings(db);
        } catch (const std::runtime_error&) {
            return true;
        }
        return false;
    }

    bool rejectsMalformedValue(const QString& path, const char* key, const char* value) {
        {
            SQLite::Database raw(path.toStdString(), SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE);
            raw.exec("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
            SQLite::Statement insert(raw, "INSERT INTO settings (key, value) VALUES (?, ?)");
            insert.bind(1, key);
            insert.bind(2, value);
            insert.exec();
        }
        try {
            Configs::Database db(path.toStdString());
            Configs::SettingsRepo settings(db);
        } catch (const std::runtime_error& error) {
            return QString::fromStdString(error.what()).contains(key);
        }
        return false;
    }

    bool reportsAllMalformedValues(const QString& path) {
        {
            SQLite::Database raw(path.toStdString(), SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE);
            raw.exec("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
            raw.exec("INSERT INTO settings VALUES ('disable_tray', 'yes'), ('test_concurrent', 'many')");
        }
        try {
            Configs::Database db(path.toStdString());
            Configs::SettingsRepo settings(db);
        } catch (const std::runtime_error& error) {
            const auto message = QString::fromStdString(error.what());
            return message.contains("disable_tray") && message.contains("test_concurrent");
        }
        return false;
    }

    // Old negative key invert-loads into private_range_bypass; new key wins if both present.
    bool loadsPrivateRangeBypassCompat(const QString& pathLegacy, const QString& pathBoth) {
        {
            SQLite::Database raw(pathLegacy.toStdString(), SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE);
            raw.exec("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
            raw.exec("INSERT INTO settings VALUES ('disable_private_range_bypass', 'true')");
        }
        {
            Configs::Database db(pathLegacy.toStdString());
            Configs::SettingsRepo loaded(db);
            // disable=true => private_range_bypass=false
            if (loaded.private_range_bypass) {
                qCritical() << "legacy invert failed: still true";
                return false;
            }
            if (!loaded.Save()) {
                qCritical() << "save after legacy load failed";
                return false;
            }
        }
        {
            SQLite::Database raw(pathLegacy.toStdString(), SQLite::OPEN_READONLY);
            SQLite::Statement q(raw, "SELECT value FROM settings WHERE key = 'private_range_bypass'");
            if (!q.executeStep()) {
                qCritical() << "private_range_bypass not written on save";
                return false;
            }
            if (std::string(q.getColumn(0).getText()) != "false") {
                qCritical() << "private_range_bypass value wrong:" << q.getColumn(0).getText();
                return false;
            }
        }

        {
            SQLite::Database raw(pathBoth.toStdString(), SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE);
            raw.exec("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
            raw.exec("INSERT INTO settings VALUES ('disable_private_range_bypass', 'true'), ('private_range_bypass', 'true')");
        }
        {
            Configs::Database db(pathBoth.toStdString());
            Configs::SettingsRepo loaded(db);
            if (!loaded.private_range_bypass) {
                qCritical() << "new key should win over legacy";
                return false;
            }
        }
        return true;
    }
}

int MessageBoxWarning(const QString&, const QString&) {
    ++warningCount;
    return 0;
}

void runOnUiThread(const std::function<void()>& callback, bool) {
    callback();
}

int main() {
    QTemporaryDir temp;
    if (!temp.isValid()) return EXIT_FAILURE;

    if (!roundTripsSettings(temp.filePath("roundtrip.db"))) return EXIT_FAILURE;
    if (!reportsWriteFailure(temp.filePath("write-failure.db"))) return EXIT_FAILURE;
    if (!skipsWriteWhenDisabled(temp.filePath("no-save.db"))) return EXIT_FAILURE;
    if (!rejectsFailedQuery(temp.filePath("failed-query.db"))) return EXIT_FAILURE;
    if (!reportsAllMalformedValues(temp.filePath("aggregate-errors.db"))) return EXIT_FAILURE;
    if (!loadsPrivateRangeBypassCompat(temp.filePath("priv-legacy.db"), temp.filePath("priv-both.db"))) {
        qCritical() << "private_range_bypass load-compat failed";
        return EXIT_FAILURE;
    }

    struct MalformedCase {
        const char* key;
        const char* value;
    };
    constexpr MalformedCase cases[] = {
        {"disable_tray", "yes"},
        {"test_concurrent", "many"},
        {"log_include_keyword", "[1]"},
        {"shortcuts", R"({"toggle":1})"},
        {"xray_vless_preference", "99"},
        {"sub_auto_update_last", "recently"},
    };
    for (int index = 0; const auto& test : cases) {
        if (!rejectsMalformedValue(temp.filePath(QString("malformed-%1.db").arg(index++)), test.key, test.value)) {
            qCritical() << "Malformed setting was accepted:" << test.key << test.value;
            return EXIT_FAILURE;
        }
    }
    return EXIT_SUCCESS;
}
