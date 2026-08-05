#include "include/database/ProfilesRepo.h"
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <map>

#include "include/database/GroupsRepo.h"
#include "include/configs/common/OutboundFactory.h"


namespace Configs {
    ProfilesRepo::ProfilesRepo(Database& database) : db(database) {
        createTables();
    }

    void ProfilesRepo::createTables() const {
        // Note: This table has a foreign key to groups(id).
        // Ensure GroupsRepo::createTables() is called before this method
        // to avoid foreign key constraint errors.
        // Create profiles table
        db.exec(R"(
            CREATE TABLE IF NOT EXISTS profiles (
                id INTEGER PRIMARY KEY,
                type TEXT NOT NULL,
                name TEXT,
                gid INTEGER NOT NULL DEFAULT 0,
                latency INTEGER NOT NULL DEFAULT 0,
                dl_speed TEXT,
                ul_speed TEXT,
                test_country TEXT,
                ip_out TEXT,
                outbound_json TEXT NOT NULL,
                traffic_dl INTEGER NOT NULL DEFAULT 0,
                traffic_up INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                FOREIGN KEY(gid) REFERENCES groups(id) ON DELETE CASCADE
            )
        )");

        // When the latency in the row was measured. Lets a consumer decide
        // whether a stored result is still worth trusting instead of guessing.
        if (!profilesColumnExists("latency_at"))
            db.exec("ALTER TABLE profiles ADD COLUMN latency_at INTEGER NOT NULL DEFAULT 0");

        // Mux A/B probe result: 0=unknown 1=yes 2=no. Default-on injects only when 1.
        if (!profilesColumnExists("mux_capability"))
            db.exec("ALTER TABLE profiles ADD COLUMN mux_capability INTEGER NOT NULL DEFAULT 0");
        if (!profilesColumnExists("mux_capability_at"))
            db.exec("ALTER TABLE profiles ADD COLUMN mux_capability_at INTEGER NOT NULL DEFAULT 0");

        db.exec("CREATE INDEX IF NOT EXISTS idx_profiles_name ON profiles(name)");
    }

    bool ProfilesRepo::profilesColumnExists(const char* columnName) const {
        auto pragma = db.query("PRAGMA table_info(profiles)");
        if (!pragma) return false;
        while (pragma->executeStep()) {
            if (pragma->getColumn(1).getText() == std::string(columnName)) return true;
        }
        return false;
    }

    QJsonObject ProfilesRepo::profileToJson(const Profile* profile) const {
        QJsonObject json;
        
        // Simple fields
        json["type"] = profile->type;
        json["name"] = profile->outbound->name;
        json["id"] = profile->id;
        json["gid"] = profile->gid;
        json["latency"] = profile->latency;
        json["latency_at"] = static_cast<qint64>(profile->latency_at);
        json["mux_capability"] = profile->mux_capability;
        json["mux_capability_at"] = static_cast<qint64>(profile->mux_capability_at);
        json["dl_speed"] = profile->dl_speed;
        json["ul_speed"] = profile->ul_speed;
        json["test_country"] = profile->test_country;
        json["ip_out"] = profile->ip_out;

        // Complex objects - serialize to JSON strings
        if (profile->outbound) {
            json["outbound"] = profile->outbound->ExportToStorageJson();
        }
        
        json["traffic_dl"] = profile->traffic_downlink.load(std::memory_order_relaxed);
        json["traffic_up"] = profile->traffic_uplink.load(std::memory_order_relaxed);
        
        return json;
    }

    std::shared_ptr<Profile> ProfilesRepo::profileFromJson(const QJsonObject& json) const {
        auto profile = std::make_shared<Profile>();
        
        // Simple fields
        profile->type = json["type"].toString();
        profile->name = json["name"].toString();
        profile->id = json["id"].toInt();
        profile->gid = json["gid"].toInt();
        profile->latency = json["latency"].toInt();
        profile->latency_at = json["latency_at"].toVariant().toLongLong();
        profile->mux_capability = json["mux_capability"].toInt();
        profile->mux_capability_at = json["mux_capability_at"].toVariant().toLongLong();
        profile->dl_speed = json["dl_speed"].toString();
        profile->ul_speed = json["ul_speed"].toString();
        profile->test_country = json["test_country"].toString();
        profile->ip_out = json["ip_out"].toString();
        
        // Reconstruct outbound (bean is not needed in new implementation)
        QString type = profile->type;
        if (type == "hysteria2") {
            type = "hysteria";
        }

        profile->outbound = Configs::NewOutboundByType(type);
        
        // Parse complex objects from JSON
        if (json.contains("outbound") && json["outbound"].isObject()) {
            profile->outbound->ParseFromJson(json["outbound"].toObject());
        }
        
        if (json.contains("traffic_dl")) profile->traffic_downlink.store(json["traffic_dl"].toVariant().toLongLong(), std::memory_order_relaxed);
        if (json.contains("traffic_up")) profile->traffic_uplink.store(json["traffic_up"].toVariant().toLongLong(), std::memory_order_relaxed);
        
        profile->name = profile->outbound->name;
        
        return profile;
    }

    void ProfilesRepo::saveToDatabase(const Profile* profile, int id) const {
        QJsonObject json = profileToJson(profile);
        QJsonDocument doc(json);
        QString jsonStr = QString::fromUtf8(doc.toJson(QJsonDocument::Compact));
        
        QString outboundJson;
        if (profile->outbound) {
            QJsonDocument outboundDoc(profile->outbound->ExportToStorageJson());
            outboundJson = QString::fromUtf8(outboundDoc.toJson(QJsonDocument::Compact));
        }
        QString name = profile->outbound ? profile->outbound->name : QString();
        const long long traffic_dl = static_cast<long long>(profile->traffic_downlink.load(std::memory_order_relaxed));
        const long long traffic_up = static_cast<long long>(profile->traffic_uplink.load(std::memory_order_relaxed));

        auto checkQuery = db.query("SELECT id FROM profiles WHERE id = ?", id);
        bool exists = checkQuery.tryStep();

        if (exists) {
            db.exec(R"(
                UPDATE profiles
                SET type = ?, name = ?, gid = ?, latency = ?, latency_at = ?,
                    mux_capability = ?, mux_capability_at = ?, dl_speed = ?, ul_speed = ?,
                    test_country = ?, ip_out = ?, outbound_json = ?,
                    traffic_dl = ?, traffic_up = ?, updated_at = strftime('%s', 'now')
                WHERE id = ?
            )",
                profile->type.toStdString(),
                name.toStdString(),
                profile->gid,
                profile->latency,
                static_cast<long long>(profile->latency_at),
                profile->mux_capability,
                static_cast<long long>(profile->mux_capability_at),
                profile->dl_speed.toStdString(),
                profile->ul_speed.toStdString(),
                profile->test_country.toStdString(),
                profile->ip_out.toStdString(),
                outboundJson.toStdString(),
                traffic_dl,
                traffic_up,
                id
            );
        } else {
            db.exec(R"(
                INSERT INTO profiles
                (id, type, name, gid, latency, latency_at, mux_capability, mux_capability_at,
                dl_speed, ul_speed, test_country, ip_out, outbound_json, traffic_dl, traffic_up)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            )",
                id,
                profile->type.toStdString(),
                name.toStdString(),
                profile->gid,
                profile->latency,
                static_cast<long long>(profile->latency_at),
                profile->mux_capability,
                static_cast<long long>(profile->mux_capability_at),
                profile->dl_speed.toStdString(),
                profile->ul_speed.toStdString(),
                profile->test_country.toStdString(),
                profile->ip_out.toStdString(),
                outboundJson.toStdString(),
                traffic_dl,
                traffic_up
            );
        }
    }

    ProfileInsertRow ProfilesRepo::profileToInsertRow(const Profile* profile, int id, int gid) const {
        QString outboundJson;
        if (profile->outbound) {
            outboundJson = QString::fromUtf8(QJsonDocument(profile->outbound->ExportToStorageJson()).toJson(QJsonDocument::Compact));
        }
        QString name = profile->outbound ? profile->outbound->name : QString();
        ProfileInsertRow row;
        row.id = id;
        row.type = profile->type.toStdString();
        row.name = name.toStdString();
        row.gid = gid;
        row.latency = profile->latency;
        row.latency_at = static_cast<long long>(profile->latency_at);
        row.mux_capability = profile->mux_capability;
        row.mux_capability_at = static_cast<long long>(profile->mux_capability_at);
        row.dl_speed = profile->dl_speed.toStdString();
        row.ul_speed = profile->ul_speed.toStdString();
        row.test_country = profile->test_country.toStdString();
        row.ip_out = profile->ip_out.toStdString();
        row.outbound_json = outboundJson.toStdString();
        row.traffic_dl = static_cast<long long>(profile->traffic_downlink.load(std::memory_order_relaxed));
        row.traffic_up = static_cast<long long>(profile->traffic_uplink.load(std::memory_order_relaxed));
        return row;
    }

    std::shared_ptr<Profile> ProfilesRepo::profileFromRow(SQLite::Statement& stmt) const {
        QJsonObject json;
        json["id"] = stmt.getColumn(0).getInt();
        json["type"] = QString::fromStdString(stmt.getColumn(1).getText());
        json["name"] = QString::fromStdString(stmt.getColumn(2).getText());
        json["gid"] = stmt.getColumn(3).getInt();
        json["latency"] = stmt.getColumn(4).getInt();
        json["latency_at"] = static_cast<qint64>(stmt.getColumn(5).getInt64());
        json["mux_capability"] = stmt.getColumn(6).getInt();
        json["mux_capability_at"] = static_cast<qint64>(stmt.getColumn(7).getInt64());
        json["dl_speed"] = QString::fromStdString(stmt.getColumn(8).getText());
        json["ul_speed"] = QString::fromStdString(stmt.getColumn(9).getText());
        json["test_country"] = QString::fromStdString(stmt.getColumn(10).getText());
        json["ip_out"] = QString::fromStdString(stmt.getColumn(11).getText());

        QString outboundJsonStr = QString::fromStdString(stmt.getColumn(12).getText());
        QJsonDocument outboundDoc = QJsonDocument::fromJson(outboundJsonStr.toUtf8());
        if (!outboundDoc.isNull() && outboundDoc.isObject()) {
            json["outbound"] = outboundDoc.object();
        }

        json["traffic_dl"] = static_cast<qint64>(stmt.getColumn(13).getInt64());
        json["traffic_up"] = static_cast<qint64>(stmt.getColumn(14).getInt64());

        return profileFromJson(json);
    }

    std::shared_ptr<Profile> ProfilesRepo::loadFromDatabase(int id) const {
        auto query = db.query(R"(
            SELECT id, type, name, gid, latency, latency_at, mux_capability, mux_capability_at,
                   dl_speed, ul_speed, test_country, ip_out, outbound_json, traffic_dl, traffic_up
            FROM profiles WHERE id = ?
        )", id);
        if (!query.tryStep()) {
            return nullptr;
        }
        return profileFromRow(*query);
    }

    std::shared_ptr<Profile> ProfilesRepo::NewProfile(const QString &type) {
        // Bean is legacy, pass nullptr
        return std::make_shared<Profile>(Configs::NewOutboundByType(type), type);
    }

    bool ProfilesRepo::AddProfile(std::shared_ptr<Profile>& profile, int gid) {
        if (profile->id >= 0) return false;
        int newId = NewProfileID();
        profile->id = newId;
        profile->gid = gid < 0 ? Configs::dataManager->settingsRepo->current_group : gid;
        QMutexLocker locker(&mutex);
        identityMap[newId] = std::weak_ptr<Profile>(profile);

        try {
            saveToDatabase(profile.get(), profile->id);
        } catch (const std::exception& e) {
            identityMap.erase(newId);
            profile->id = -1;
            qWarning() << "ProfilesRepo::AddProfile DB write failed:" << e.what();
            return false;
        }

        if (auto group = dataManager->groupsRepo->GetGroup(profile->gid)) {
            group->AddProfile(profile->id);
            dataManager->groupsRepo->Save(group);
        } else {
            identityMap.erase(newId);
            profile->id = -1;
            return false;
        }
        return true;
    }

    bool ProfilesRepo::AddProfileBatch(QList<std::shared_ptr<Profile>>& profiles, int gid) {
        gid = gid < 0 ? Configs::dataManager->settingsRepo->current_group : gid;
        auto group = dataManager->groupsRepo->GetGroup(gid);
        if (!group) return false;

        QList<std::shared_ptr<Profile>> toAdd;
        for (auto& profile : profiles) {
            if (profile && profile->id < 0) toAdd.append(profile);
        }
        if (toAdd.isEmpty()) return true;

        const int n = toAdd.size();
        const int firstId = NewProfileIDRange(n);
        const QList<int> oldIds = group->profiles;

        QList<int> newIds = oldIds;
        newIds.reserve(oldIds.size() + n);
        std::vector<ProfileInsertRow> rows;
        rows.reserve(static_cast<size_t>(n));

        QMutexLocker locker(&mutex);
        for (int i = 0; i < n; ++i) {
            const int id = firstId + i;
            toAdd[i]->id = id;
            toAdd[i]->gid = gid;
            identityMap[id] = std::weak_ptr<Profile>(toAdd[i]);
            newIds << id;
            rows.push_back(profileToInsertRow(toAdd[i].get(), id, gid));
        }

        // Provisionally rewrite group list; restore on failure (same shape as Replace).
        group->profiles = newIds;

        // Held across the catch as well, so no other thread can slip in between a
        // failed statement and the ROLLBACK that undoes it.
        auto connLock = db.lockConnection();
        try {
            db.execThrow("BEGIN IMMEDIATE");
            db.execBatchInsertProfilesThrow(rows);

            QJsonArray columnWidthArray = QListInt2QJsonArray(group->column_width);
            QJsonArray profilesArray = QListInt2QJsonArray(group->profiles);
            const QString columnWidthJson = QString::fromUtf8(QJsonDocument(columnWidthArray).toJson(QJsonDocument::Compact));
            const QString profilesJson = QString::fromUtf8(QJsonDocument(profilesArray).toJson(QJsonDocument::Compact));
            db.execThrow(R"(
                UPDATE groups
                SET archive = ?, skip_auto_update = ?, auto_clear_unavailable = ?, name = ?, url = ?, info = ?,
                    sub_last_update = ?, front_proxy_id = ?, landing_proxy_id = ?,
                    column_width_json = ?, profiles_json = ?, scroll_last_profile = ?, test_sort_by = ?, traffic_sort_by = ?, test_items_to_show = ?,
                    type_sort_by = ?,
                    updated_at = strftime('%s', 'now')
                WHERE id = ?
            )",
                group->archive ? 1 : 0,
                group->skip_auto_update ? 1 : 0,
                group->auto_clear_unavailable ? 1 : 0,
                group->name.toStdString(),
                group->url.toStdString(),
                group->info.toStdString(),
                static_cast<long long>(group->sub_last_update),
                group->front_proxy_id,
                group->landing_proxy_id,
                columnWidthJson.toStdString(),
                profilesJson.toStdString(),
                group->scroll_last_profile,
                static_cast<int>(group->test_sort_by),
                static_cast<int>(group->traffic_sort_by),
                static_cast<int>(group->test_items_to_show),
                static_cast<int>(group->type_sort_by),
                gid
            );
            db.execThrow("COMMIT");
        } catch (std::exception& e) {
            try { db.execThrow("ROLLBACK"); } catch (...) {}
            group->profiles = oldIds;
            for (int i = 0; i < n; ++i) {
                identityMap.erase(toAdd[i]->id);
                toAdd[i]->id = -1;
                toAdd[i]->gid = -1;
            }
            NotifyError("AddProfileBatch", e);
            return false;
        }

        return true;
    }

    std::shared_ptr<Profile> ProfilesRepo::GetProfile(int id) const {
        QMutexLocker locker(&mutex);
        if (auto it = identityMap.find(id); it != identityMap.end()) {
            if (auto shared = it->second.lock()) return shared;
            identityMap.erase(it);
        }
        auto profile = loadFromDatabase(id);
        if (!profile) return nullptr;
        identityMap[id] = std::weak_ptr<Profile>(profile);
        return profile;
    }

    std::map<int, std::shared_ptr<Profile>> ProfilesRepo::loadProfilesByIdsChunk(const QList<int>& chunkIds) const {
        std::map<int, std::shared_ptr<Profile>> result;
        if (chunkIds.isEmpty()) return result;
        QString idList;
        for (int i = 0; i < chunkIds.size(); ++i) {
            if (i > 0) idList += ",";
            idList += QString::number(chunkIds[i]);
        }
        std::string sql = "SELECT id, type, name, gid, latency, latency_at, mux_capability, mux_capability_at, "
                         "dl_speed, ul_speed, test_country, ip_out, outbound_json, traffic_dl, traffic_up "
                         "FROM profiles WHERE id IN (" +
                         idList.toStdString() + ") ORDER BY id";
        auto query = db.query(sql);
        if (!query) return result;
        while (query.tryStep()) {
            auto profile = profileFromRow(*query);
            result[profile->id] = std::move(profile);
        }
        return result;
    }

    QList<std::shared_ptr<Profile>> ProfilesRepo::GetProfileBatch(QList<int> ids) {
        QList<std::shared_ptr<Profile>> profiles;
        if (ids.isEmpty()) return profiles;

        std::map<int, std::shared_ptr<Profile>> byId;
        QList<int> missingIds;
        QMutexLocker locker(&mutex);
        for (int id : ids) {
            auto it = identityMap.find(id);
            if (it != identityMap.end()) {
                if (auto shared = it->second.lock()) {
                    byId[id] = shared;
                    continue;
                }
                identityMap.erase(it);
            }
            missingIds.append(id);
        }
        if (missingIds.isEmpty()) {
            for (int id : ids) {
                auto it = byId.find(id);
                if (it != byId.end()) profiles.push_back(it->second);
            }
            return profiles;
        }

        for (int off = 0; off < missingIds.size(); off += Configs::BATCH_LIMIT_READ) {
            int end = std::min(off + Configs::BATCH_LIMIT_READ, static_cast<int>(missingIds.size()));
            auto chunk = missingIds.sliced(off, end - off);
            std::map<int, std::shared_ptr<Profile>> loaded = loadProfilesByIdsChunk(chunk);
            for (auto& p : loaded) byId[p.first] = std::move(p.second);
        }
        for (const auto& p : byId) {
            identityMap[p.first] = std::weak_ptr<Profile>(p.second);
        }
        for (int id : ids) {
            auto it = byId.find(id);
            if (it != byId.end()) profiles.push_back(it->second);
        }
        return profiles;
    }

    QList<std::pair<int, QString> > ProfilesRepo::GetProfileIDNameMappedBatch(QList<int> ids) {
        QList<std::pair<int, QString> > result;
        if (ids.isEmpty()) return result;

        std::map<int, QString> idToName;

        for (int off = 0; off < ids.size(); off += Configs::BATCH_LIMIT_READ) {
            const int end = std::min(off + Configs::BATCH_LIMIT_READ, static_cast<int>(ids.size()));
            const auto chunk = ids.sliced(off, end - off);
            if (chunk.isEmpty()) continue;

            QString idList;
            for (int i = 0; i < chunk.size(); ++i) {
                if (i > 0) idList += ",";
                idList += QString::number(chunk[i]);
            }
            const std::string sql = "SELECT id, name FROM profiles WHERE id IN (" + idList.toStdString() + ") ORDER BY id";
            auto query = db.query(sql);
            if (!query) continue;
            while (query.tryStep()) {
                const int id = query->getColumn(0).getInt();
                idToName[id] = QString::fromStdString(query->getColumn(1).getText());
            }
        }

        for (int id : ids) {
            const auto it = idToName.find(id);
            if (it != idToName.end()) {
                result.append({it->first, it->second});
            }
        }
        return result;
    }

    std::shared_ptr<Profile> ProfilesRepo::GetProfileByName(const QString& name) {
        // Query by name using the index, extract id and destroy statement before GetProfile
        int id;
        {
            auto query = db.query("SELECT id FROM profiles WHERE name = ? LIMIT 1", name.toStdString());
            if (!query.tryStep()) {
                return nullptr;
            }
            id = query->getColumn(0).getInt();
        }
        // Statement destroyed here, DB lock released before acquiring repo mutex
        return GetProfile(id);
    }

    QList<std::pair<int, QString> > ProfilesRepo::GetAllProfileIDNameMapped() {
        auto query = db.query("SELECT id, name FROM profiles ORDER BY id");
        if (!query) return {};
        QList<std::pair<int, QString> > res;
        while (query.tryStep()) {
            res.append({query->getColumn(0).getInt(), QString(query->getColumn(1).getString().c_str())});
        }
        return res;
    }

    QStringList ProfilesRepo::GetAllProfileNames() {
        auto query = db.query("SELECT name FROM profiles ORDER BY id");
        if (!query) return {};
        QStringList names;
        while (query.tryStep()) {
            names.append(QString(query->getColumn(0).getString().c_str()));
        }
        return names;
    }

        bool ProfilesRepo::BatchDeleteProfiles(QList<int>& ids, bool stopRunningProfile, bool* outDeletedRunningProfile) {
        QSet<int> groupIDs;
        if (ids.contains(dataManager->settingsRepo->started_id)) {
            if (stopRunningProfile) {
                // The UI layer owns stopping the running profile; report that
                // the deletion set contains it instead of calling into the UI.
                if (outDeletedRunningProfile) *outDeletedRunningProfile = true;
            } else {
                ids.removeAll(dataManager->settingsRepo->started_id);
            }
        }
        auto profiles = GetProfileBatch(ids);
        for (const auto& ent : profiles) {
            groupIDs.insert(ent->gid);
        }
        for (auto groupID : groupIDs) {
            auto group = dataManager->groupsRepo->GetGroup(groupID);
            if (!group) {
                qWarning() << "BatchDeleteProfiles: could not find group with id" << groupID;
                return false;
            }
            group->RemoveProfileBatch(ids);
            dataManager->groupsRepo->Save(group);
        }
        QMutexLocker locker(&mutex);
        for (int id : ids) identityMap.erase(id);
        if (!ids.isEmpty()) {
            std::vector<int> idVec(ids.begin(), ids.end());
            db.execDeleteByIdIn("profiles", "id", idVec);
        }
        return true;
    }

    bool ProfilesRepo::ReplaceGroupProfiles(int gid, QList<std::shared_ptr<Profile>>& newProfiles,
                                            const QList<QPair<int, int>>& keep) {
        auto group = dataManager->groupsRepo->GetGroup(gid);
        if (!group) return false;

        QList<std::shared_ptr<Profile>> toAdd;
        for (auto& profile : newProfiles) {
            if (profile && profile->id < 0) toAdd.append(profile);
        }
        // Empty result is rejected: keep existing nodes.
        if (toAdd.isEmpty()) return false;

        const QList<int> groupIdsBefore = group->profiles;
        QList<int> oldIds = groupIdsBefore;
        for (const auto& [position, id] : keep) oldIds.removeAll(id);
        const int n = toAdd.size();
        const int firstId = NewProfileIDRange(n);

        QList<int> newIds;
        newIds.reserve(n);
        std::vector<ProfileInsertRow> rows;
        rows.reserve(static_cast<size_t>(n));

        QMutexLocker locker(&mutex);
        for (int i = 0; i < n; ++i) {
            const int id = firstId + i;
            toAdd[i]->id = id;
            toAdd[i]->gid = gid;
            identityMap[id] = std::weak_ptr<Profile>(toAdd[i]);
            newIds << id;
            rows.push_back(profileToInsertRow(toAdd[i].get(), id, gid));
        }

        for (const auto& [position, id] : keep) {
            newIds.insert(std::min<qsizetype>(position, newIds.size()), id);
        }

        // Provisionally rewrite group list; restore on failure.
        group->profiles = newIds;

        // Held across the catch as well, so no other thread can slip in between a
        // failed statement and the ROLLBACK that undoes it.
        auto connLock = db.lockConnection();
        try {
            db.execThrow("BEGIN IMMEDIATE");
            db.execBatchInsertProfilesThrow(rows);

            // Persist group metadata + profiles_json in the same transaction.
            QJsonArray columnWidthArray = QListInt2QJsonArray(group->column_width);
            QJsonArray profilesArray = QListInt2QJsonArray(group->profiles);
            const QString columnWidthJson = QString::fromUtf8(QJsonDocument(columnWidthArray).toJson(QJsonDocument::Compact));
            const QString profilesJson = QString::fromUtf8(QJsonDocument(profilesArray).toJson(QJsonDocument::Compact));
            db.execThrow(R"(
                UPDATE groups
                SET archive = ?, skip_auto_update = ?, auto_clear_unavailable = ?, name = ?, url = ?, info = ?,
                    sub_last_update = ?, front_proxy_id = ?, landing_proxy_id = ?,
                    column_width_json = ?, profiles_json = ?, scroll_last_profile = ?, test_sort_by = ?, traffic_sort_by = ?, test_items_to_show = ?,
                    type_sort_by = ?,
                    updated_at = strftime('%s', 'now')
                WHERE id = ?
            )",
                group->archive ? 1 : 0,
                group->skip_auto_update ? 1 : 0,
                group->auto_clear_unavailable ? 1 : 0,
                group->name.toStdString(),
                group->url.toStdString(),
                group->info.toStdString(),
                static_cast<long long>(group->sub_last_update),
                group->front_proxy_id,
                group->landing_proxy_id,
                columnWidthJson.toStdString(),
                profilesJson.toStdString(),
                group->scroll_last_profile,
                static_cast<int>(group->test_sort_by),
                static_cast<int>(group->traffic_sort_by),
                static_cast<int>(group->test_items_to_show),
                static_cast<int>(group->type_sort_by),
                gid
            );

            if (!oldIds.isEmpty()) {
                std::vector<int> oldVec(oldIds.begin(), oldIds.end());
                db.execDeleteByIdInThrow("profiles", "id", oldVec);
            }
            db.execThrow("COMMIT");
        } catch (std::exception& e) {
            try { db.execThrow("ROLLBACK"); } catch (...) {}
            // Revert memory state so callers keep the previous working set.
            group->profiles = groupIdsBefore;
            for (int i = 0; i < n; ++i) {
                identityMap.erase(toAdd[i]->id);
                toAdd[i]->id = -1;
                toAdd[i]->gid = -1;
            }
            NotifyError("ReplaceGroupProfiles", e);
            return false;
        }

        for (int id : oldIds) identityMap.erase(id);
        return true;
    }

    QList<int> ProfilesRepo::GetAllProfileIds() const {
        QList<int> ids;
        auto query = db.query("SELECT id FROM profiles ORDER BY id");
        if (query) {
            while (query.tryStep()) {
                ids.append(query->getColumn(0).getInt());
            }
        }
        return ids;
    }

    QList<int> ProfilesRepo::GetProfileIdsByType(const QString& type) const {
        QList<int> ids;
        auto query = db.query("SELECT id FROM profiles WHERE type = ? ORDER BY id", type.toStdString());
        if (query) {
            while (query->executeStep()) {
                ids.append(query->getColumn(0).getInt());
            }
        }
        return ids;
    }

    int ProfilesRepo::NewProfileID() const {
        // Atomically increment and get the new ID using RETURNING clause (DB atomic, no lock required)
        auto query = db.query("UPDATE entity_ids SET profile_last_id = profile_last_id + 1 RETURNING profile_last_id");
        if (query.tryStep()) {
            return query->getColumn(0).getInt();
        }
        return 0;
    }

    int ProfilesRepo::NewProfileIDRange(int n) const {
        if (n <= 0) return 0;
        // Atomically reserve n IDs; RETURNING gives the new value (old + n), so first ID = newValue - n + 1
        auto query = db.query("UPDATE entity_ids SET profile_last_id = profile_last_id + ? RETURNING profile_last_id", n);
        if (query.tryStep()) {
            int newValue = query->getColumn(0).getInt();
            return newValue - n + 1;
        }
        return 0;
    }

    bool ProfilesRepo::Save(const std::shared_ptr<Profile>& profile) {
        if (!profile || profile->id < 0) {
            return false;
        }
        
        QMutexLocker locker(&mutex);
        saveToDatabase(profile.get(), profile->id);
        identityMap[profile->id] = std::weak_ptr<Profile>(profile);
        
        return true;
    }

    bool ProfilesRepo::SaveTraffic(const std::shared_ptr<Profile>& profile) {
        if (!profile || profile->id < 0) {
            return false;
        }
        const int id = profile->id;
        const long long dl = static_cast<long long>(profile->traffic_downlink.load(std::memory_order_relaxed));
        const long long up = static_cast<long long>(profile->traffic_uplink.load(std::memory_order_relaxed));
        runOnNewThread([=, this] {
            db.exec("UPDATE profiles SET traffic_dl = ?, traffic_up = ? WHERE id = ?", dl, up, id);
        });
        return true;
    }

    void ProfilesRepo::SaveTrafficBatch(const QList<std::shared_ptr<Profile>>& profiles) {
        QList<std::shared_ptr<Profile>> valid;
        for (const auto& p : profiles) {
            if (p && p->id >= 0) valid.append(p);
        }
        if (valid.isEmpty()) return;
        // Snapshot once per profile so concurrent TrafficLooper writes don't tear
        // the pair mid-SQL.
        QList<QPair<int, QPair<long long, long long>>> snapshots;
        snapshots.reserve(valid.size());
        for (const auto& p : valid) {
            snapshots.append({p->id, {
                static_cast<long long>(p->traffic_downlink.load(std::memory_order_relaxed)),
                static_cast<long long>(p->traffic_uplink.load(std::memory_order_relaxed)),
            }});
        }
        QMutexLocker locker(&mutex);
        // Held across the catch as well, so no other thread can slip in between a
        // failed statement and the ROLLBACK that undoes it.
        auto connLock = db.lockConnection();
        try {
            db.execThrow("BEGIN IMMEDIATE");
            for (const auto& s : snapshots) {
                db.execThrow("UPDATE profiles SET traffic_dl = ?, traffic_up = ? WHERE id = ?",
                             s.second.first, s.second.second, s.first);
            }
            db.execThrow("COMMIT");
        } catch (std::exception& e) {
            try { db.execThrow("ROLLBACK"); } catch (...) {}
            NotifyError("SaveTrafficBatch", e);
        }
    }

    void ProfilesRepo::SaveBatch(const QList<std::shared_ptr<Profile>>& profiles) {
        runOnNewThread([=, this] {
            QList<std::shared_ptr<Profile>> valid;
            for (const auto& p : profiles) {
                if (p && p->id >= 0) valid.append(p);
            }
            if (valid.isEmpty()) return;
            std::vector<ProfileInsertRow> rows;
            rows.reserve(valid.size());
            for (const auto& p : valid) {
                rows.push_back(profileToInsertRow(p.get(), p->id, p->gid));
            }
            QMutexLocker locker(&mutex);
            db.execBatchReplaceProfiles(rows);
            for (const auto& p : valid) {
                identityMap[p->id] = std::weak_ptr<Profile>(p);
            }
        });
    }
}
