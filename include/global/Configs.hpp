#pragma once

#include "Const.hpp"
#include "Utils.hpp"
#include <QFileInfo>
#include "include/database/DatabaseManager.h"
#include <srslist.h>

// Switch core support

namespace Configs {
    void initDB(const std::string& dbPath);

    QString FindCoreRealPath();

    bool IsAdmin(bool forceRenew=false);

    bool isSetuidSet(const std::string& path);

    QString GetBasePath();

    // AdBlock rule-set asset (same dir as geoip.dat). Start uses local only.
    inline QString adblockRulesetFileName() {
        return QStringLiteral("adblocksingbox.srs");
    }
    inline QString adblockRulesetPath() {
        return GetBasePath() + QStringLiteral("/") + adblockRulesetFileName();
    }
    inline bool adblockRulesetAvailable() {
        return QFileInfo::exists(adblockRulesetPath());
    }
} // namespace Configs

#define ROUTES_PREFIX_NAME QString("route_profiles")
#define ROUTES_PREFIX QString(ROUTES_PREFIX_NAME + "/")
