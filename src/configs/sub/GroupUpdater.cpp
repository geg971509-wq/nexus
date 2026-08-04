#include "include/database/entities/Profile.h"
#include "include/global/HTTPRequestHelper.hpp"

#include "include/configs/sub/GroupUpdater.hpp"
#include "include/configs/sub/clash.hpp"

#include <QInputDialog>
#include <QUrlQuery>
#include <QJsonDocument>
#include <QHash>

#include "include/configs/common/utils.h"
#include "include/database/GroupsRepo.h"
#include "include/database/ProfilesRepo.h"
#include "include/ui/mainwindow.h"

namespace Subscription {

    namespace {
        bool PrepareProfilesForDeletion(QList<int>& ids) {
            const auto& settings = Configs::dataManager->settingsRepo;
            if (!ids.contains(settings->started_id)) return true;
            if (settings->allow_stopping_active_profile) {
                return GetMainWindow()->profile_stop(false, true, false);
            } else {
                ids.removeAll(settings->started_id);
            }
            return true;
        }

        // Shared payload handling for "json://" links and "throne://add/" deep
        // links: base64 -> JSON object -> typed profile. Returns nullptr when
        // the payload is empty, is not a JSON object, or names an unknown type.
        std::shared_ptr<Configs::Profile> profileFromBase64Json(const QByteArray &dataBytes) {
            if (dataBytes.isEmpty()) return nullptr;
            auto data = QJsonDocument::fromJson(dataBytes).object();
            if (data.isEmpty()) return nullptr;
            std::shared_ptr<Configs::Profile> ent;
            if (data.contains("protocol")) {
                ent = Configs::ProfilesRepo::NewProfile("xray" + data["protocol"].toString());
            } else {
                ent = data["type"].toString() == "hysteria2" ? Configs::ProfilesRepo::NewProfile("hysteria") : Configs::ProfilesRepo::NewProfile(data["type"].toString());
            }
            if (ent->outbound->invalid) return nullptr;
            ent->outbound->ParseFromJson(data);
            return ent;
        }

        // VLESS inputs that use Xray-only features become xrayvless profiles.
        std::shared_ptr<Configs::Profile> vlessFromLink(const QString &str) {
            if (Configs::useXrayVless(str)) {
                auto ent = Configs::ProfilesRepo::NewProfile("xrayvless");
                return ent->XrayVLESS()->ParseFromLink(str) ? ent : nullptr;
            }
            auto ent = Configs::ProfilesRepo::NewProfile("vless");
            return ent->VLESS()->ParseFromLink(str) ? ent : nullptr;
        }

        std::shared_ptr<Configs::Profile> vlessFromClash(const clash::Proxies &out) {
            if (out.network == "xhttp" || (!out.encryption.empty() && out.encryption != "none")) {
                auto ent = Configs::ProfilesRepo::NewProfile("xrayvless");
                return ent->XrayVLESS()->ParseFromClash(out) ? ent : nullptr;
            }
            auto ent = Configs::ProfilesRepo::NewProfile("vless");
            return ent->VLESS()->ParseFromClash(out) ? ent : nullptr;
        }

        // One row per supported profile protocol, shared by all three
        // subscription parsers: share-link schemes (RawUpdater::update),
        // sing-box outbound "type" values (updateSingBox) and Clash proxy
        // "type" values (updateClash). Adding a protocol is a one-row change.
        struct ProtocolDispatch {
            const char *profileType; // Configs::ProfilesRepo::NewProfile argument
            std::initializer_list<const char *> linkPrefixes;
            std::initializer_list<const char *> singBoxTypes;
            std::initializer_list<const char *> clashTypes;
            // Per-input overrides for when the profile type depends on content.
            std::function<std::shared_ptr<Configs::Profile>(const QString &)> customFromLink = nullptr;
            std::function<std::shared_ptr<Configs::Profile>(const clash::Proxies &)> customFromClash = nullptr;
        };

        const ProtocolDispatch kProtocolDispatchTable[] = {
            // profileType  link prefixes                                          sing-box types             clash types
            {"socks",       {"socks5://", "socks4://", "socks4a://", "socks://"},  {"socks"},                 {"socks5"}},
            {"http",        {"http://", "https://"},                                 {"http"},                  {"http"}},
            {"shadowsocks", {"ss://"},                                               {"shadowsocks"},           {"ss"}},
            {"vmess",       {"vmess://"},                                            {"vmess"},                 {"vmess"}},
            {"vless",       {"vless://"},                                            {"vless"},                 {"vless"}, vlessFromLink, vlessFromClash},
            {"trojan",      {"trojan://"},                                           {"trojan"},                {"trojan"}},
            {"anytls",      {"anytls://"},                                           {"anytls"},                {"anytls"}},
            // mierus:// is the "simple" sharing link; the base64 "standard"
            // mieru:// link is rejected inside ParseFromLink rather than mis-parsed.
            {"mieru",       {"mierus://", "mieru://"},                               {"mieru"},                {}},
            {"hysteria",    {"hysteria://", "hysteria2://", "hy2://"},               {"hysteria", "hysteria2"}, {"hysteria", "hysteria2"}},
            {"tuic",        {"tuic://"},                                             {"tuic"},                  {"tuic"}},
            {"juicity",     {"juicity://"},                                          {"juicity"},               {}},
            {"trusttunnel", {"tt://"},                                               {"trusttunnel"},           {}},
            {"shadowtls",   {"shadowtls://"},                                        {"shadowtls"},             {}},
            {"wireguard",   {"wg://"},                                               {"wireguard"},             {}},
            {"ssh",         {"ssh://"},                                              {"ssh"},                   {"ssh"}},
            {"naive",       {"naive+https://", "naive+quic://"},                     {"naive"},                 {}},
        };

        const ProtocolDispatch *findProtocolByLink(const QString &str) {
            for (const auto &pd : kProtocolDispatchTable)
                for (const char *prefix : pd.linkPrefixes)
                    if (str.startsWith(QLatin1StringView(prefix))) return &pd;
            return nullptr;
        }

        const ProtocolDispatch *findProtocolBySingBoxType(const QString &type) {
            for (const auto &pd : kProtocolDispatchTable)
                for (const char *t : pd.singBoxTypes)
                    if (type == QLatin1StringView(t)) return &pd;
            return nullptr;
        }

        const ProtocolDispatch *findProtocolByClashType(const std::string &type) {
            for (const auto &pd : kProtocolDispatchTable)
                for (const char *t : pd.clashTypes)
                    if (type == t) return &pd;
            return nullptr;
        }

        // Handlers return nullptr on parse failure, matching the historical
        // skip-this-entry behavior of the three call sites.
        std::shared_ptr<Configs::Profile> parseLinkProtocol(const ProtocolDispatch &pd, const QString &str) {
            if (pd.customFromLink) return pd.customFromLink(str);
            auto ent = Configs::ProfilesRepo::NewProfile(pd.profileType);
            return ent->outbound->ParseFromLink(str) ? ent : nullptr;
        }

        std::shared_ptr<Configs::Profile> parseSingBoxProtocol(const ProtocolDispatch &pd, const QJsonObject &out) {
            auto ent = Configs::ProfilesRepo::NewProfile(pd.profileType);
            return ent->outbound->ParseFromJson(out) ? ent : nullptr;
        }

        std::shared_ptr<Configs::Profile> parseClashProtocol(const ProtocolDispatch &pd, const clash::Proxies &out) {
            if (pd.customFromClash) return pd.customFromClash(out);
            auto ent = Configs::ProfilesRepo::NewProfile(pd.profileType);
            return ent->outbound->ParseFromClash(out) ? ent : nullptr;
        }
    }

    GroupUpdater *groupUpdater = new GroupUpdater;

    SingBoxSubType getSingBoxSubType(const QJsonDocument &doc) {
        if (doc.isObject()) {
            auto obj = doc.object();
            bool hasInbound = obj.contains("inbounds");
            bool hasOutbound = obj.contains("outbounds") || obj.contains("endpoints");
            // if (hasInbound && hasOutbound) return SingBoxSubType::fullConfig;
            if (hasOutbound) return SingBoxSubType::outboundInJson;
            if (obj.contains("type")) return SingBoxSubType::outboundObject;
            return SingBoxSubType::invalid;
        }
        if (doc.isArray() && !doc.array().empty()) {
            auto arr = doc.array();
            auto firstRaw = arr.first();
            if (firstRaw.isObject()) {
                auto obj = firstRaw.toObject();
                if (obj.contains("type")) return SingBoxSubType::outboundJsonArray;
            }
            return SingBoxSubType::invalid;
        }
        return SingBoxSubType::invalid;
    }

    // Xray uses "protocol" instead of sing-box's "type" field on outbounds, so
    // we can disambiguate by inspecting individual outbound objects rather than
    // the wrapper.
    XraySubType getXraySubType(const QJsonDocument &doc) {
        if (doc.isObject()) {
            auto obj = doc.object();
            if (obj.contains("outbounds")) {
                for (const auto &item : obj["outbounds"].toArray()) {
                    if (item.isObject() && item.toObject().contains("protocol")) {
                        return XraySubType::outboundInJson;
                    }
                }
            }
            if (obj.contains("protocol")) return XraySubType::outboundObject;
            return XraySubType::invalid;
        }
        if (doc.isArray() && !doc.array().empty()) {
            auto first = doc.array().first();
            if (first.isObject()) {
                auto obj = first.toObject();
                // Array of bare outbounds (each tagged with "protocol").
                if (obj.contains("protocol")) return XraySubType::outboundJsonArray;
                // Array of complete Xray configs (the "Xray JSON subscription"
                // format): each element carries an "outbounds" array of its own.
                // Require a "protocol"-tagged outbound so this only matches Xray
                // configs, not sing-box ones (whose outbounds use "type").
                if (obj.contains("outbounds")) {
                    for (const auto &item : obj["outbounds"].toArray()) {
                        if (item.isObject() && item.toObject().contains("protocol")) {
                            return XraySubType::configJsonArray;
                        }
                    }
                }
            }
        }
        return XraySubType::invalid;
    }

    // Convert a real Xray VLESS outbound (settings.vnext[0].address etc.) into
    // the simplified shape Throne's xrayVless::ParseFromJson expects. Returns
    // an empty object if the input doesn't have the expected structure.
    QJsonObject normalizeXrayVlessForParse(const QJsonObject &out) {
        if (out["protocol"].toString() != "vless") return {};
        auto settings = out["settings"].toObject();
        // Already in simplified form.
        if (settings.contains("address") && !settings.contains("vnext")) return out;
        auto vnext = settings["vnext"].toArray();
        if (vnext.isEmpty()) return {};
        auto first = vnext.first().toObject();
        if (first.isEmpty()) return {};
        auto users = first["users"].toArray();
        if (users.isEmpty()) return {};
        auto user = users.first().toObject();
        QJsonObject simpleSettings;
        simpleSettings["address"] = first["address"];
        simpleSettings["port"] = first["port"];
        simpleSettings["id"] = user["id"];
        simpleSettings["encryption"] = user.contains("encryption") ? user["encryption"] : QJsonValue("none");
        simpleSettings["flow"] = user["flow"];
        QJsonObject normalized = out;
        normalized["settings"] = simpleSettings;
        return normalized;
    }

    std::shared_ptr<Configs::Profile> makeProfileForXrayOutbound(const QJsonObject &out) {
        if (out.isEmpty()) return nullptr;
        auto protocol = out["protocol"].toString();
        // System protocols don't make sense as user profiles.
        if (protocol == "freedom" || protocol == "blackhole" || protocol == "dns" || protocol == "loopback") {
            return nullptr;
        }
        std::shared_ptr<Configs::Profile> ent;
        if (protocol == "vless") {
            if (auto normalized = normalizeXrayVlessForParse(out); !normalized.isEmpty()) {
                ent = Configs::ProfilesRepo::NewProfile("xrayvless");
                if (ent->XrayVLESS()->ParseFromJson(normalized)) return ent;
            }
        }
        ent = Configs::ProfilesRepo::NewProfile("custom");
        ent->Custom()->type = Configs::Custom::CustomXrayOutbound;
        ent->Custom()->config = QJsonObject2QString(out, false);
        if (auto tag = out["tag"].toString(); !tag.isEmpty()) ent->Custom()->name = tag;
        return ent;
    }

    void RawUpdater::update(const QString &str, bool needParse, bool isBase64Decoded) {
        // Base64 encoded subscription
        if (!isBase64Decoded) {
            if (auto str2 = DecodeB64IfValid(str); !str2.isEmpty()) {
                update(str2, true, true);
                return;
            }
        }

        std::shared_ptr<Configs::Profile> ent;

        // Json
        QJsonParseError error;
        auto doc = QJsonDocument::fromJson(str.toUtf8(), &error);
        if (error.error == QJsonParseError::NoError) {
            // Xray (checked first since its outbounds are tagged with
            // "protocol", which lets us cleanly disambiguate from sing-box
            // configs that share the "outbounds" wrapper).
            auto xrayType = getXraySubType(doc);
            if (xrayType == XraySubType::outboundObject) {
                if (auto e = makeProfileForXrayOutbound(doc.object()); e != nullptr) {
                    updated_order += e;
                }
                return;
            }
            if (xrayType == XraySubType::outboundInJson || xrayType == XraySubType::outboundJsonArray ||
                xrayType == XraySubType::configJsonArray) {
                updateXray(doc, xrayType);
                return;
            }

            // SingBox
            auto subType = getSingBoxSubType(doc);
            if (subType == SingBoxSubType::fullConfig) {
                ent = Configs::ProfilesRepo::NewProfile("custom");
                ent->Custom()->type = Configs::Custom::CustomFullConfig;
                ent->Custom()->config = str;
                updated_order += ent;
            } else if (subType == SingBoxSubType::outboundObject) {
                ent = Configs::ProfilesRepo::NewProfile("custom");
                ent->Custom()->type = Configs::Custom::CustomOutbound;
                ent->Custom()->config = str;
                updated_order += ent;
            } else if (subType == SingBoxSubType::outboundInJson || subType == SingBoxSubType::outboundJsonArray) {
                updateSingBox(doc, subType);
                return;
            }

            // SIP008
            if (str.contains("version") && str.contains("servers"))
            {
                updateSIP008(str);
                return;
            }

            return;
        }

        const auto trimmed = str.trimmed();
        if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
            MW_show_log(QObject::tr("Invalid JSON subscription: %1 (offset %2)")
                            .arg(error.errorString()).arg(error.offset));
            return;
        }

        // Clash
        if (str.contains("proxies:")) {
            updateClash(str);
            return;
        }

        // Wireguard Config
        if (str.contains("[Interface]") && str.contains("[Peer]"))
        {
            updateWireguardFileConfig(str);
            return;
        }

        // Multi line
        if (str.count("\n") > 0 && needParse) {
            for (const auto &line : str.split('\n', Qt::SkipEmptyParts)) {
                update(line.trimmed(), false);
            }
            return;
        }

        // is comment or too short
        if (str.startsWith("//") || str.startsWith("#") || str.length() < 2) {
            return;
        }

        // Json base64 link format
        if (str.startsWith("json://")) {
            auto link = QUrl(str);
            if (!link.isValid()) return;
            ent = profileFromBase64Json(DecodeB64IfValid(link.fragment().toUtf8(), QByteArray::Base64UrlEncoding));
            if (ent == nullptr) return;
        }

        // throne://add/ deep link
        if (str.startsWith("throne://add/", Qt::CaseInsensitive)) {
            auto link = QUrl(str);
            if (!link.isValid()) return;
            ent = profileFromBase64Json(DecodeB64IfValid(link.path().mid(1)));
            if (ent == nullptr) return;
        }

        // Json
        if (str.startsWith('{')) {
            ent = Configs::ProfilesRepo::NewProfile("custom");
            auto custom = ent->Custom();
            auto obj = QString2QJsonObject(str);
            if (obj.contains("outbounds")) {
                custom->type = Configs::Custom::CustomFullConfig;
                custom->config = str;
            } else if (obj.contains("server")) {
                custom->type = Configs::Custom::CustomOutbound;
                custom->config = str;
            } else {
                return;
            }
        }

        // Protocol share links (see kProtocolDispatchTable)
        if (const auto *pd = findProtocolByLink(str)) {
            ent = parseLinkProtocol(*pd, str);
            if (ent == nullptr) return;
        }

        if (ent == nullptr) return;

        // End
        updated_order += ent;
    }

    void RawUpdater::updateSingBox(const QJsonDocument &doc, SingBoxSubType type)
    {
        QJsonArray outbounds, endpoints;
        if (type == SingBoxSubType::outboundInJson) {
            auto json = doc.object();
            outbounds = json["outbounds"].toArray();
            endpoints = json["endpoints"].toArray();
        } else if (type == SingBoxSubType::outboundJsonArray) {
            outbounds = doc.array();
        } else {
            return;
        }
        QJsonArray items;
        for (const auto& outbound : outbounds)
        {
            if (!outbound.isObject()) continue;
            items.append(outbound.toObject());
        }
        for (const auto& endpoint : endpoints)
        {
            if (!endpoint.isObject()) continue;
            items.append(endpoint.toObject());
        }

        for (const auto& o : items)
        {
            auto out = o.toObject();
            if (out.isEmpty())
            {
                MW_show_log(QStringLiteral("invalid outbound of type: %1").arg(static_cast<int>(o.type())));
                continue;
            }

            const auto *pd = findProtocolBySingBoxType(out["type"].toString());
            if (pd == nullptr) continue;

            auto ent = parseSingBoxProtocol(*pd, out);
            if (ent == nullptr) continue;

            updated_order += ent;
        }
    }

    void RawUpdater::updateXray(const QJsonDocument &doc, XraySubType type)
    {
        // "Xray JSON subscription": an array of complete, self-contained Xray
        // configs. Each element carries its own inbounds/outbounds/routing and
        // often relies on balancers and dialerProxy chains between its
        // outbounds, so it can't be flattened into individual proxies without
        // losing that logic. Import each as a CustomXrayFullConfig — the whole
        // config runs verbatim as Throne's Xray instance behind a socks bridge.
        if (type == XraySubType::configJsonArray) {
            for (const auto &c : doc.array()) {
                if (!c.isObject()) continue;
                auto cfg = c.toObject();
                if (!cfg.contains("outbounds")) continue;
                // Drop the subscription's own client inbounds (typically socks
                // 10808 / http 10809). Throne injects its own bridge inbound at
                // build time and routes everything through it; the bundled
                // inbounds are never in the traffic path and would only risk
                // port-bind conflicts. Safe here because none of these configs'
                // routing rules match on inboundTag.
                cfg.remove("inbounds");
                auto ent = Configs::ProfilesRepo::NewProfile("custom");
                ent->Custom()->type = Configs::Custom::CustomXrayFullConfig;
                ent->Custom()->config = QJsonObject2QString(cfg, false);
                if (auto remarks = cfg["remarks"].toString(); !remarks.isEmpty()) ent->Custom()->name = remarks;
                updated_order += ent;
            }
            return;
        }

        QJsonArray outbounds;
        if (type == XraySubType::outboundInJson) {
            outbounds = doc.object()["outbounds"].toArray();
        } else if (type == XraySubType::outboundJsonArray) {
            outbounds = doc.array();
        } else {
            return;
        }
        for (const auto &o : outbounds) {
            if (!o.isObject()) continue;
            if (auto e = makeProfileForXrayOutbound(o.toObject()); e != nullptr) {
                updated_order += e;
            }
        }
    }

    void RawUpdater::updateClash(const QString& str)
    {
        fkyaml::node node;
        clash::Clash clash_config;

        try {
            node = fkyaml::node::deserialize(str.toStdString());
            clash_config = node.get_value<clash::Clash>();
        } catch (const std::exception& e) {
            MW_show_log(QObject::tr("Clash YAML parse error: %1").arg(e.what()));
            return;
        }

        for (const auto& out : clash_config.proxies)
        {
            try {
                const auto *pd = findProtocolByClashType(out.type);
                if (pd == nullptr) continue;

                auto ent = parseClashProtocol(*pd, out);
                if (ent == nullptr) continue;

                updated_order += ent;
            } catch (const std::exception& e) {
                // Per-proxy isolation: skip bad proxy, log, continue with remaining
                MW_show_log(QObject::tr("Skipping malformed proxy: %1").arg(e.what()));
                continue;
            }
        }
    }

    void RawUpdater::updateWireguardFileConfig(const QString& str)
    {
        auto ent = Configs::ProfilesRepo::NewProfile("wireguard");
        auto ok = ent->Wireguard()->ParseFromLink(str);
        if (!ok) return;
        updated_order += ent;
    }

    void RawUpdater::updateSIP008(const QString& str)
    {
        auto json = QString2QJsonObject(str);

        for (const auto& o : json["servers"].toArray())
        {
            auto out = o.toObject();
            if (out.isEmpty())
            {
                MW_show_log("invalid server object");
                continue;
            }

            auto ent = Configs::ProfilesRepo::NewProfile("shadowsocks");
            auto ok = ent->ShadowSocks()->ParseFromSIP008(out);
            if (!ok) continue;
            updated_order += ent;
        }
    }

    // 在新的 thread 运行
    void GroupUpdater::AsyncUpdate(const QString &str, int _sub_gid, const std::function<void()> &finish, bool showDiff) {
        auto content = str.trimmed();
        bool asURL = false;
        bool createNewGroup = false;
        int targetGid = _sub_gid;

        if (_sub_gid < 0 && (content.startsWith("http://") || content.startsWith("https://"))) {
            auto items = QStringList{
                QObject::tr("Add profiles to this group"),
                QObject::tr("Create new subscription group"),
                QObject::tr("Import HTTP proxy profile"),
            };
            bool ok;
            auto a = QInputDialog::getItem(nullptr,
                                           QObject::tr("url detected"),
                                           QObject::tr("%1\nHow to update?").arg(content),
                                           items, 0, false, &ok);
            if (!ok) return;
            switch (items.indexOf(a)) {
                case 1: createNewGroup = true;
                case 0: asURL = true; break;
            }
            if (asURL && !createNewGroup) {
                targetGid = Configs::dataManager->settingsRepo->current_group;
                auto group = Configs::dataManager->groupsRepo->GetGroup(targetGid);
                if (group == nullptr) return;
                group->url = content;
                Configs::dataManager->groupsRepo->Save(group);
            }
        }

        runOnNewThread([=,this] {
            auto gid = targetGid;
            if (createNewGroup) {
                auto group = Configs::GroupsRepo::NewGroup();
                group->name = QUrl(str).host();
                group->url = str;
                if (!Configs::dataManager->groupsRepo->AddGroup(group)) {
                    MW_show_log(QObject::tr("Failed to add the group for subscription: %1").arg(str));
                    return;
                }
                gid = group->id;
                MW_dialog_message(MwMessage::SubscriptionNewGroup, {});
            }
            Update(str, gid, asURL, showDiff);
            emit asyncUpdateCallback(gid);
            if (finish) {
                QMetaObject::invokeMethod(this, [finish] { finish(); }, Qt::QueuedConnection);
            }
        });
    }

    void GroupUpdater::Update(const QString &_str, int _sub_gid, bool _not_sub_as_url, bool showDiff) {
        // 创建 rawUpdater
        Configs::dataManager->settingsRepo->imported_count = 0;
        auto rawUpdater = std::make_unique<RawUpdater>();
        rawUpdater->gid_add_to = _sub_gid;

        // 准备
        QString sub_user_info;
        bool asURL = _sub_gid >= 0 || _not_sub_as_url; // 把 _str 当作 url 处理（下载内容）
        auto content = _str.trimmed();
        auto group = Configs::dataManager->groupsRepo->GetGroup(_sub_gid);
        if (group != nullptr && group->archive) return;

        // 网络请求
        if (asURL) {
            auto groupName = group == nullptr ? content : group->name;
            MW_show_log(">>>>>>>> " + QObject::tr("Requesting subscription: %1").arg(groupName));

            auto resp = NetworkRequestHelper::HttpGet(content, Configs::dataManager->settingsRepo->sub_send_hwid);
            if (!resp.error.isEmpty()) {
                MW_show_log("<<<<<<<< " + QObject::tr("Requesting subscription %1 error: %2").arg(groupName, resp.error + "\n" + resp.data));
                return;
            }

            content = resp.data;
            sub_user_info = NetworkRequestHelper::GetHeader(resp.header, "Subscription-UserInfo");

            MW_show_log("<<<<<<<< " + QObject::tr("Subscription request fininshed: %1").arg(groupName));
        }

        const auto jsonCandidate = content.trimmed();
        if (group != nullptr && (jsonCandidate.startsWith('{') || jsonCandidate.startsWith('['))) {
            QJsonParseError error;
            QJsonDocument::fromJson(jsonCandidate.toUtf8(), &error);
            if (error.error != QJsonParseError::NoError) {
                MW_show_log(QObject::tr("Invalid JSON subscription: %1 (offset %2)")
                                .arg(error.errorString()).arg(error.offset));
                return;
            }
        }

        QList<std::shared_ptr<Configs::Profile>> in;
        // Profiles the subscription does not own and must never touch. An auto
        // selector is local state that tracks the group rather than a server the
        // remote sent us, so leaving it in the diff would report it as removed
        // on every single refresh and then delete it. Positions are kept so a
        // refresh does not shuffle the group either.
        QList<QPair<int, int>> sticky; // (position in the group, profile id)
        QSet<int> stickyIDs;
        // Profiles this refresh invalidated: deleted outright, or kept under the
        // same id with different settings. A running auto selector that built
        // any of them can no longer trust its config.
        QList<int> disturbed;

        if (group != nullptr) {
            group->sub_last_update = QDateTime::currentMSecsSinceEpoch() / 1000;
            group->info = sub_user_info;
            // Metadata above is persisted with the atomic replace (sub_clear) or
            // after a successful non-clear import below. Avoid writing group state
            // before the profile set is known to be valid.
            for (int i = 0; i < group->profiles.size(); i++) {
                auto ent = Configs::dataManager->profilesRepo->GetProfile(group->profiles[i]);
                if (ent == nullptr || ent->type != "autoselector") continue;
                sticky << qMakePair(i, group->profiles[i]);
                stickyIDs.insert(group->profiles[i]);
            }
            if (!Configs::dataManager->settingsRepo->sub_clear) {
                for (const auto &ent : Configs::dataManager->profilesRepo->GetProfileBatch(group->Profiles())) {
                    if (ent != nullptr && !stickyIDs.contains(ent->id)) in << ent;
                }
            }
        }

        MW_show_log(">>>>>>>> " + QObject::tr("Processing subscription data..."));
        rawUpdater->update(content);
        content.clear();

        if (group != nullptr && Configs::dataManager->settingsRepo->sub_clear) {
            MW_show_log(QObject::tr("Clearing servers..."));
            QList<int> oldIds;
            for (int id : group->profiles) {
                if (!stickyIDs.contains(id)) oldIds << id;
            }
            // Reject empty parse results: keep existing nodes.
            if (rawUpdater->updated_order.isEmpty()) {
                MW_show_log(QObject::tr("Subscription produced no profiles; keeping existing servers."));
                // Persist only metadata change when content was valid but empty.
                Configs::dataManager->groupsRepo->Save(group);
                MW_dialog_message(MwMessage::SubscriptionFinished, {MwArg::Quiet});
                return;
            }
            // May drop the started id from oldIds when it must not be stopped.
            if (!PrepareProfilesForDeletion(oldIds)) return;
            // Whatever the replace is not deleting has to be carried over, or it
            // would be dropped from the group anyway: the sticky auto selectors
            // plus the started profile when it was spared above.
            QList<QPair<int, int>> keep;
            for (int i = 0; i < group->profiles.size(); i++) {
                if (!oldIds.contains(group->profiles[i])) keep << qMakePair(i, group->profiles[i]);
            }
            disturbed = oldIds;
            if (!Configs::dataManager->profilesRepo->ReplaceGroupProfiles(group->id, rawUpdater->updated_order, keep)) {
                runOnUiThread([=] {
                    MessageBoxWarning("Internal Error", "DB Error when replacing profiles, Please try again.");
                });
                return;
            }
        } else {
            // Reject empty parse results: keep existing nodes.
            if (rawUpdater->updated_order.isEmpty()) {
                MW_show_log(QObject::tr("Subscription produced no profiles; keeping existing servers."));
                if (group != nullptr) {
                    Configs::dataManager->groupsRepo->Save(group);
                }
                MW_dialog_message(MwMessage::SubscriptionFinished, {MwArg::Quiet});
                return;
            }
            if (group != nullptr) {
                Configs::dataManager->groupsRepo->Save(group);
            }
            if (!Configs::dataManager->profilesRepo->AddProfileBatch(rawUpdater->updated_order, rawUpdater->gid_add_to)) {
                MW_show_log(QObject::tr("DB Error when adding profiles, Please try again."));
                return;
            }
        }
        MW_show_log(">>>>>>>> " + QObject::tr("Process complete, applying..."));

        if (group != nullptr) {
            QList<std::shared_ptr<Configs::Profile>> out_all;
            for (const auto &ent : Configs::dataManager->profilesRepo->GetProfileBatch(group->Profiles())) {
                if (ent != nullptr && !stickyIDs.contains(ent->id)) out_all << ent;
            }

            QString change_text;

            if (Configs::dataManager->settingsRepo->sub_clear) {
                // all is new profile
                if (out_all.size() >= 1000) {
                    change_text += "[+] " + Int2String(out_all.size()) + " profiles\n";
                } else {
                    for (const auto &ent: out_all) {
                        change_text += "[+] " + ent->outbound->DisplayTypeAndName() + "\n";
                    }
                }
            } else {
                QList<std::shared_ptr<Configs::Profile>> update_keep;
                QList<std::shared_ptr<Configs::Profile>> update_del;
                QList<std::shared_ptr<Configs::Profile>> only_out;
                QList<std::shared_ptr<Configs::Profile>> only_in;
                QList<std::shared_ptr<Configs::Profile>> out;
                // find and delete not updated profile by ProfileFilter
                Configs::ProfileFilter::OnlyInSrc_ByPointer(out_all, in, out);
                Configs::ProfileFilter::OnlyInSrc(in, out, only_in, false);
                Configs::ProfileFilter::OnlyInSrc(out, in, only_out, false);
                Configs::ProfileFilter::Common(in, out, update_keep, update_del, false);

                QList<std::shared_ptr<Configs::Profile>> changed_old;
                QList<std::shared_ptr<Configs::Profile>> changed_new;
                Configs::ProfileFilter::ChangedByIdentity(only_in, only_out, changed_old, changed_new);

                QString notice_added;
                QString notice_deleted;
                QString notice_updated;
                if (only_out.size() < 1000)
                {
                    for (const auto &ent: only_out) {
                        notice_added += "[+] " + ent->outbound->DisplayTypeAndName() + "\n";
                    }
                } else
                {
                    notice_added += QString("[+] ") + "added " + Int2String(only_out.size()) + "\n";
                }
                if (changed_new.size() < 1000)
                {
                    for (const auto &ent: changed_new) {
                        notice_updated += "[~] " + ent->outbound->DisplayTypeAndName() + "\n";
                    }
                } else
                {
                    notice_updated += QString("[~] ") + "updated " + Int2String(changed_new.size()) + "\n";
                }
                if (only_in.size() < 1000)
                {
                    for (const auto &ent: only_in) {
                        notice_deleted += "[-] " + ent->outbound->DisplayTypeAndName() + "\n";
                    }
                } else
                {
                    notice_deleted += QString("[-] ") + "deleted " + Int2String(only_in.size()) + "\n";
                }


                QHash<Configs::Profile *, int> supersededBy;
                for (int i = 0; i < update_del.size() && i < update_keep.size(); ++i) {
                    supersededBy[update_del[i].get()] = update_keep[i]->id;
                }
                for (int i = 0; i < changed_new.size(); ++i) {
                    const auto &oldEnt = changed_old[i];
                    oldEnt->outbound = changed_new[i]->outbound;
                    oldEnt->name = oldEnt->outbound->name;
                    Configs::dataManager->profilesRepo->Save(oldEnt);
                    supersededBy[changed_new[i].get()] = oldEnt->id;
                    // Same id, different server: anything already running on it
                    // is now working from a stale config.
                    disturbed << oldEnt->id;
                }

                // sort according to order in remote
                group->profiles.clear();
                for (const auto &ent: rawUpdater->updated_order) {
                    auto it = supersededBy.find(ent.get());
                    if (it != supersededBy.end()) {
                        group->profiles.append(it.value());
                    } else {
                        group->profiles.append(ent->id);
                    }
                }
                for (const auto &[position, id] : sticky) {
                    group->profiles.insert(std::min<qsizetype>(position, group->profiles.size()), id);
                }
                Configs::dataManager->groupsRepo->Save(group);

                // cleanup
                QList<int> del_ids;
                for (const auto &ent: out_all) {
                    if (!group->HasProfile(ent->id)) {
                        del_ids.append(ent->id);
                    }
                }
                if (!PrepareProfilesForDeletion(del_ids)) return;
                disturbed << del_ids;
                if (!Configs::dataManager->profilesRepo->BatchDeleteProfiles(del_ids)) {
                    runOnUiThread([=] {
                       MessageBoxWarning("Internal error", "DB Error when deleting profiles, data may be corrupted");
                    });
                }

                change_text = "\n" + QObject::tr("Added %1 profiles:\n%2\nUpdated %3 profiles:\n%4\nDeleted %5 Profiles:\n%6")
                                         .arg(only_out.length())
                                         .arg(notice_added)
                                         .arg(changed_old.length())
                                         .arg(notice_updated)
                                         .arg(only_in.length())
                                         .arg(notice_deleted);
                if (only_out.length() + only_in.length() + changed_old.length() == 0) change_text = QObject::tr("Nothing");
            }

            MW_show_log("<<<<<<<< " + QObject::tr("Change of %1:").arg(group->name) + "\n" + change_text);
            if (showDiff && Configs::dataManager->settingsRepo->sub_show_change_popup) {
                // Manual refresh: surface the same diff in a popup, not just the log.
                const auto diffTitle = QObject::tr("Change of %1").arg(group->name);
                auto diffBody = change_text.trimmed();
                if (diffBody.isEmpty()) diffBody = QObject::tr("Nothing");
                runOnUiThread([diffTitle, diffBody] { MessageBoxScrollable(diffTitle, diffBody); });
            }
            // Auto selectors resolve their members from the group at build time,
            // so a refresh can invalidate one without ever touching the profile
            // itself. Hand over what changed and let the main window decide
            // whether a running selector has to be rebuilt.
            QStringList selectorArgs{Int2String(group->id)};
            for (int id : disturbed) selectorArgs << Int2String(id);
            MW_dialog_message(MwMessage::SubscriptionGroupChanged, selectorArgs);
            MW_dialog_message(MwMessage::SubscriptionFinished, {MwArg::Quiet});
        } else {
            Configs::dataManager->settingsRepo->imported_count = rawUpdater->updated_order.count();
            MW_dialog_message(MwMessage::SubscriptionFinished, {});
        }
    }
} // namespace Subscription

bool UI_update_all_groups_Updating = false;

#define should_skip_group(g) (g == nullptr || g->url.isEmpty() || g->archive || (onlyAllowed && g->skip_auto_update))

void serialUpdateSubscription(const QList<int> &groupsTabOrder, int _order, bool onlyAllowed) {
    if (_order >= groupsTabOrder.size()) {
        UI_update_all_groups_Updating = false;
        return;
    }

    // calculate this group
    auto group = Configs::dataManager->groupsRepo->GetGroup(groupsTabOrder[_order]);
    if (group == nullptr || should_skip_group(group)) {
        serialUpdateSubscription(groupsTabOrder, _order + 1, onlyAllowed);
        return;
    }

    int nextOrder = _order + 1;
    while (nextOrder < groupsTabOrder.size()) {
        auto nextGid = groupsTabOrder[nextOrder];
        auto nextGroup = Configs::dataManager->groupsRepo->GetGroup(nextGid);
        if (!should_skip_group(nextGroup)) {
            break;
        }
        nextOrder += 1;
    }

    // Async update current group
    UI_update_all_groups_Updating = true;
    Subscription::groupUpdater->AsyncUpdate(group->url, group->id, [=] {
        serialUpdateSubscription(groupsTabOrder, nextOrder, onlyAllowed);
    });
}

void UI_update_all_groups(bool onlyAllowed) {
    if (UI_update_all_groups_Updating) {
        MW_show_log("The last subscription update has not exited.");
        return;
    }

    auto groupsTabOrder = Configs::dataManager->groupsRepo->GetGroupsTabOrder();
    serialUpdateSubscription(groupsTabOrder, 0, onlyAllowed);
}
