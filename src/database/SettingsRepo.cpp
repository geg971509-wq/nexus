#include "include/database/SettingsRepo.h"
#include <QJsonDocument>
#include <QJsonArray>
#include <QMutexLocker>
#include <stdexcept>

#include "include/global/Utils.hpp"

namespace Configs {
    SettingsRepo::SettingsRepo(Database& database) : db(database) {
        initMaps();
        createTables();
        loadAllSettings();
    }

    void SettingsRepo::initMaps() {
        boolMap = {
            {"disable_tray",                  &disable_tray},
            {"random_inbound_port",           &random_inbound_port},
            {"mux_padding",                   &mux_padding},
            {"mux_default_on",                &mux_default_on},
            {"fragment_default_on",           &fragment_default_on},
            {"tls_tricks_default_on",         &tls_tricks_default_on},
            {"net_use_proxy",                 &net_use_proxy},
            {"remember_enable",               &remember_enable},
            {"skip_cert",                     &skip_cert},
            {"fakedns",                       &fake_dns},
            {"disable_traffic_stats",         &disable_traffic_stats},
            {"disable_traffic_aggregation",   &disable_traffic_aggregation},
            {"vpn_ipv6",                      &vpn_ipv6},
            {"vpn_strict_route",              &vpn_strict_route},
            {"vpn_auto_redirect",             &vpn_auto_redirect},
            {"sub_clear",                     &sub_clear},
            {"sub_show_change_popup",         &sub_show_change_popup},
            {"net_insecure",                  &net_insecure},
            {"sub_send_hwid",                 &sub_send_hwid},
            {"start_minimal",                 &start_minimal},
            {"enable_ntp",                    &enable_ntp},
            {"enable_dns_server",             &enable_dns_server},
            {"dns_server_listen_lan",         &dns_server_listen_lan},
            {"enable_redirect",               &enable_redirect},
            {"system_dns_set",                &system_dns_set},
            {"windows_set_admin",             &windows_set_admin},
            {"disable_win_admin",             &disable_run_admin},
            {"enable_stats",                  &enable_stats},
            {"disable_privilege_req",         &disable_privilege_req},
            {"enable_tun_routing",            &enable_tun_routing},
            {"use_mozilla_certs",             &use_mozilla_certs},
            {"allow_beta_update",             &allow_beta_update},
            {"adblock_enable",                &adblock_enable},
            {"show_system_dns",               &show_system_dns},
            {"use_custom_icons",              &use_custom_icons},
            {"xray_mux_default_on",           &xray_mux_default_on},
            {"use_dns_object",                &use_dns_object},
            {"skip_delete_confirmation",      &skip_delete_confirmation},
            {"show_config_security",          &show_config_security},
            {"log_enable_include",            &log_enable_include},
            {"log_enable_exclude",            &log_enable_exclude},
            {"log_auto_scroll",               &log_auto_scroll},
            {"enable_warp",                   &enable_warp},
            {"enable_dns_routing",            &enable_dns_routing},
            {"inbound_auth",                  &inbound_auth},
            {"allow_stopping_active_profile", &allow_stopping_active_profile},
            {"disable_mixed_inbound",         &disable_mixed_inbound},
            {"system_proxy_enabled",          &remember_system_proxy},
            {"tun_mode_enabled",              &remember_tun},
            {"session_system_proxy",          &session_system_proxy},
            {"session_vpn",                   &session_vpn},
            {"reset_proxy_on_disable_sp", &reset_proxy_on_disable_sp},
            {"dns_disable_cache", &dns_disable_cache},
            {"dns_disable_expire", &dns_disable_expire},
            {"dns_reverse_mapping", &dns_reverse_mapping},
            {"private_range_bypass", &private_range_bypass},
        };

        intMap = {
            {"current_group",          &current_group},
            {"last_filter_column",     &last_filter_column},
            {"inbound_socks_port",     &inbound_socks_port},
            {"mux_concurrency",        &mux_concurrency},
            {"test_concurrent",        &test_concurrent},
            {"remember_id",            &remember_id},
            {"language",               &language},
            {"font_size",              &font_size},
            {"max_log_line",           &max_log_line},
            {"stats_tab",              &stats_tab},
            {"traffic_stats_retention_days", &traffic_stats_retention_days},
            {"sub_auto_update",        &sub_auto_update},
            {"route_auto_update",      &route_auto_update},
            {"vpn_mtu",                &vpn_mtu},
            {"ntp_server_port",        &ntp_server_port},
            {"dns_server_listen_port", &dns_server_listen_port},
            {"redirect_listen_port",   &redirect_listen_port},
            {"core_box_clash_api",     &core_box_clash_api},
            {"speed_test_mode",        &speed_test_mode},
            {"speed_test_timeout_ms",  &speed_test_timeout_ms},
            {"url_test_timeout_ms",    &url_test_timeout_ms},
            {"xray_mux_concurrency",   &xray_mux_concurrency},
            {"current_route_id",       &current_route_id},
            {"ruleset_mirror",         &ruleset_mirror},
            {"core_dns_in_port",       &core_dns_in_port},
            {"dns_cache_capacity", &dns_cache_capacity},
        };

        stringMap = {
            {"user_agent2",                &user_agent},
            {"test_url",                   &test_latency_url},
            {"inbound_address",            &inbound_address},
            {"log_level",                  &log_level},
            {"mux_protocol",               &mux_protocol},
            {"fragment_implementation",    &fragment_implementation},
            {"fragment_size",              &fragment_size},
            {"fragment_sleep",             &fragment_sleep},
            {"theme",                      &theme},
            {"custom_inbound",             &custom_inbound},
            {"custom_route",               &custom_route_global},
            {"font",                       &font},
            {"hk_mw",                      &hotkey_mainwindow},
            {"hk_group",                   &hotkey_group},
            {"hk_route",                   &hotkey_route},
            {"hk_spmenu",                  &hotkey_system_proxy_menu},
            {"hk_toggle",                  &hotkey_toggle_system_proxy},
            {"active_routing",             &active_routing},
            {"mw_size",                    &mw_size},
            {"vpn_impl",                   &vpn_implementation},
            {"vpn_tun_ipv4_cidr",          &vpn_tun_ipv4_cidr},
            {"vpn_tun_ipv6_cidr",          &vpn_tun_ipv6_cidr},
            {"sub_custom_hwid_params",     &sub_custom_hwid_params},
            {"splitter_state",             &splitter_state},
            {"utlsFingerprint",            &utlsFingerprint},
            {"core_box_clash_listen_addr", &core_box_clash_listen_addr},
            {"core_box_clash_api_secret",  &core_box_clash_api_secret},
            {"core_box_underlying_dns",    &core_box_underlying_dns},
            {"ntp_server_address",         &ntp_server_address},
            {"ntp_interval",               &ntp_interval},
            {"ntp_outbound",               &ntp_outbound},
            {"dns_v4_resp",                &dns_v4_resp},
            {"dns_v6_resp",                &dns_v6_resp},
            {"redirect_listen_address",    &redirect_listen_address},
            {"proxy_scheme",               &proxy_scheme},
            {"main_window_geometry",       &mainWindowGeometry},
            {"xray_log_level",             &xray_log_level},
            {"xray_geoip_url",             &xray_geoip_url},
            {"xray_geosite_url",           &xray_geosite_url},
            {"remote_dns",                 &remote_dns},
            {"remote_dns_strategy",        &remote_dns_strategy},
            {"direct_dns",                 &direct_dns},
            {"direct_dns_strategy",        &direct_dns_strategy},
            {"dns_object",                 &dns_object},
            {"dns_final_out",              &dns_final_out},
            {"domain_strategy",            &resolve_domain_strategy},
            {"outbound_domain_strategy",   &default_domain_strategy},
            {"simple_dl_url",              &simple_dl_url},
            {"warp_private_key",           &warp_private_key},
            {"warp_public_key",            &warp_public_key},
            {"warp_ep",                    &warp_ep},
            {"warp_device_id",             &warp_device_id},
            {"warp_access_token",          &warp_access_token},
            {"warp_license",               &warp_license},
            {"warp_local_public_key",      &warp_local_public_key},
            {"warp_account_state",         &warp_account_state},
            {"inbound_user",               &inbound_user},
            {"inbound_pass",               &inbound_pass},
            {"url_scheme_mirror",          &url_scheme_mirror},
        };

        stringListMap = {
            {"dns_server_rules",         &dns_server_rules},
            {"extra_core_paths",         &extraCorePaths},
            {"log_include_keyword",      &log_include_keyword},
            {"log_include_regex",        &log_include_regex},
            {"log_exclude_keyword",      &log_exclude_keyword},
            {"log_exclude_regex",        &log_exclude_regex},
            {"warp_ifc_addrs",           &warp_ifc_addrs},
            {"dial_bind_ifc_history",    &dial_bind_interface_history},
            {"dial_inet4_bind_history",  &dial_inet4_bind_address_history},
            {"dial_inet6_bind_history",  &dial_inet6_bind_address_history},
            {"warp_reserved", &warp_reserved},
        };
    }

    void SettingsRepo::createTables() const {
        db.exec(R"(
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
        )");
    }

    void SettingsRepo::loadAllSettings() {
        auto query = db.query("SELECT key, value FROM settings");
        if (!query) throw std::runtime_error("Failed to load settings from the database");

        QStringList errors;
        const auto reject = [&errors](const QString& key, const QString& expected, const QString& value) {
            constexpr qsizetype maxValueLength = 80;
            const auto bounded = value.size() > maxValueLength ? value.left(maxValueLength) + "..." : value;
            errors << QString("%1: expected %2, got '%3'").arg(key, expected, bounded);
        };

        // Old negative key: invert into private_range_bypass unless the new key also appears.
        bool sawPrivateRangeBypass = false;
        bool legacyDisablePrivateRangeBypass = false;
        bool sawLegacyDisablePrivateRangeBypass = false;

        while (query->executeStep()) {
            const QString key = QString::fromStdString(query->getColumn(0).getText());
            const QString str = QString::fromStdString(query->getColumn(1).getText());

            if (key == "private_range_bypass") {
                sawPrivateRangeBypass = true;
            }
            if (key == "disable_private_range_bypass") {
                // Not in boolMap anymore; load-compat only.
                if (str == "true" || str == "1") {
                    legacyDisablePrivateRangeBypass = true;
                    sawLegacyDisablePrivateRangeBypass = true;
                } else if (str == "false" || str == "0") {
                    legacyDisablePrivateRangeBypass = false;
                    sawLegacyDisablePrivateRangeBypass = true;
                } else {
                    reject(key, "boolean (true, false, 1, or 0)", str);
                }
                continue;
            }

            if (auto boolVal = boolMap.find(key); boolVal != boolMap.end()) {
                if (str == "true" || str == "1") *boolVal.value() = true;
                else if (str == "false" || str == "0") *boolVal.value() = false;
                else reject(key, "boolean (true, false, 1, or 0)", str);
            } else if (auto intVal = intMap.find(key); intVal != intMap.end()) {
                bool ok = false;
                const auto value = str.toInt(&ok);
                if (ok) *intVal.value() = value;
                else reject(key, "integer", str);
            } else if (auto strListVal = stringListMap.find(key); strListVal != stringListMap.end()) {
                const QJsonDocument doc = QJsonDocument::fromJson(str.toUtf8());
                if (doc.isArray()) {
                    QStringList list;
                    bool valid = true;
                    for (const auto& val : doc.array()) {
                        if (!val.isString()) {
                            valid = false;
                            break;
                        }
                        list << val.toString();
                    }
                    if (valid) *strListVal.value() = list;
                    else reject(key, "JSON array of strings", str);
                } else reject(key, "JSON array of strings", str);
            } else if (auto strVal = stringMap.find(key); strVal != stringMap.end()) {
                *strVal.value() = str;
            } else if (key == "shortcuts") {
                const QJsonDocument doc = QJsonDocument::fromJson(str.toUtf8());
                if (doc.isObject()) {
                    const auto obj = doc.object();
                    QMap<QString, QKeySequence> parsed;
                    bool valid = true;
                    for (const auto& shortcutKey : obj.keys()) {
                        if (!obj[shortcutKey].isString()) {
                            valid = false;
                            break;
                        }
                        parsed[shortcutKey] = QKeySequence(obj[shortcutKey].toString());
                    }
                    if (valid) shortcuts = parsed;
                    else reject(key, "JSON object with string values", str);
                } else reject(key, "JSON object with string values", str);
            } else if (key == "xray_vless_preference") {
                bool ok = false;
                const int value = str.toInt(&ok);
                if (ok && value >= Xray::XhttpOnly && value <= Xray::AllVLESS) {
                    xray_vless_preference = static_cast<Xray::XrayVlessPreference>(value);
                } else reject(key, "valid Xray VLESS preference", str);
            } else if (key == "sub_auto_update_last") {
                bool ok = false;
                const auto value = str.toLongLong(&ok);
                if (ok) sub_auto_update_last = value;
                else reject(key, "64-bit integer timestamp", str);
            } else if (key == "route_auto_update_last") {
                bool ok = false;
                const auto value = str.toLongLong(&ok);
                if (ok) route_auto_update_last = value;
                else reject(key, "64-bit integer timestamp", str);
            }
        }

        if (!sawPrivateRangeBypass && sawLegacyDisablePrivateRangeBypass) {
            private_range_bypass = !legacyDisablePrivateRangeBypass;
        }

        if (!errors.isEmpty()) {
            throw std::runtime_error(QString("Invalid stored settings: %1").arg(errors.join("; ")).toStdString());
        }
    }

    bool SettingsRepo::saveAllSettings() const {
        if (noSave) return true;

        std::vector<std::pair<std::string, std::string>> keyValues;
        keyValues.reserve(boolMap.size() + intMap.size() + stringMap.size() + stringListMap.size() + 4);

        for (auto it = boolMap.begin(); it != boolMap.end(); ++it)
            keyValues.emplace_back(it.key().toStdString(), *it.value() ? "true" : "false");

        for (auto it = intMap.begin(); it != intMap.end(); ++it)
            keyValues.emplace_back(it.key().toStdString(), QString::number(*it.value()).toStdString());

        for (auto it = stringMap.begin(); it != stringMap.end(); ++it)
            keyValues.emplace_back(it.key().toStdString(), it.value()->toStdString());

        for (auto it = stringListMap.begin(); it != stringListMap.end(); ++it) {
            QJsonArray arr;
            for (const QString& s : *it.value()) arr.append(s);
            keyValues.emplace_back(it.key().toStdString(),
                QString::fromUtf8(QJsonDocument(arr).toJson(QJsonDocument::Compact)).toStdString());
        }

        {
            QJsonObject obj;
            for (auto it = shortcuts.begin(); it != shortcuts.end(); ++it)
                obj[it.key()] = it.value().toString();
            keyValues.emplace_back("shortcuts",
                QString::fromUtf8(QJsonDocument(obj).toJson(QJsonDocument::Compact)).toStdString());
        }

        keyValues.emplace_back("xray_vless_preference",
            QString::number(static_cast<int>(xray_vless_preference)).toStdString());

        // qint64 last-run timestamps for the periodic auto-update jobs (out of range for
        // the int map, so persisted here alongside the other special cases).
        keyValues.emplace_back("sub_auto_update_last",
            QString::number(sub_auto_update_last).toStdString());
        keyValues.emplace_back("route_auto_update_last",
            QString::number(route_auto_update_last).toStdString());

        return db.execBatchSettingsReplace(keyValues);
    }

    void SettingsRepo::UpdateStartedId(int id) {
        started_id = id;
        remember_id = id;
        Save();
    }

    QString SubStrBefore(QString str, const QString &sub) {
        if (!str.contains(sub)) return str;
        return str.left(str.indexOf(sub));
    }

    QString SettingsRepo::GetUserAgent(bool isDefault) const {
        if (user_agent.isEmpty()) {
            isDefault = true;
        }
        if (isDefault) {
            QString version = SubStrBefore(NKR_VERSION, "-");
            if (!version.contains(".")) version = "1.0.0";
            return "Throne/" + version;
        }
        return user_agent;
    }

    bool SettingsRepo::Save() {
        return saveAllSettings();
    }

    QStringList SettingsRepo::GetExtraCorePaths() const {
        return extraCorePaths;
    }

    bool SettingsRepo::AddExtraCorePath(const QString &path) {
        if (extraCorePaths.contains(path)) {
            return false;
        }
        extraCorePaths.append(path);
        return true;
    }
}
