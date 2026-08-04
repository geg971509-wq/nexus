#pragma once
#include <QJsonArray>
#include <QJsonObject>

#include "include/database/entities/Profile.h"

namespace Configs
{
    class ExtraCoreData
    {
        public:
        QString path;
        QString args;
        QString config;
        bool noLog;
    };

    class DNSDeps
    {
        public:
        bool needDirectDnsRules = false;
        QJsonArray directDomains;
        QJsonArray directRuleSets;
        QJsonArray directSuffixes;
        QJsonArray directKeywords;
        QJsonArray directRegexes;
        bool needProxyDnsRules = false;
        QJsonArray proxyDomains;
        QJsonArray proxyRuleSets;
        QJsonArray proxySuffixes;
        QJsonArray proxyKeywords;
        QJsonArray proxyRegexes;
    };

    class HijackDeps
    {
        public:
        QJsonArray hijackDomains;
        QJsonArray hijackDomainSuffix;
        QJsonArray hijackDomainRegex;
        QJsonArray hijackGeoAssets;

        // A DNS rule whose only condition is query_type matches *every* query of
        // that type, so with no conditions at all no rule may be emitted.
        [[nodiscard]] bool noConditions() const
        {
            return hijackDomains.isEmpty() && hijackDomainSuffix.isEmpty() &&
                   hijackDomainRegex.isEmpty() && hijackGeoAssets.isEmpty();
        }
    };

    class TunDeps
    {
        public:
        QJsonArray directIPSets;
        QJsonArray directIPCIDRs;
    };

    class RoutingDeps
    {
        public:
        int defaultOutboundID;
        QList<int> neededOutbounds;       // kept for compatibility but no longer consumed
        QStringList neededRuleSets;
        std::map<int, QString> outboundMap;
        // One routing outbound group. hopIDs is the list of profile IDs to
        // build outbounds for: single profile -> [id], chain -> [outerHop,
        // ..., innerHop] (reversed, matching existing chain build order).
        // chainWrapper is set when the route rule's referenced outbound was a
        // chain, so traffic accounting can also credit the wrapper (which
        // isn't in hopIDs); nullptr otherwise.
        struct RouteOutboundGroup {
            QList<int> hopIDs;
            std::shared_ptr<Profile> chainWrapper;
        };
        QList<RouteOutboundGroup> routeOutboundGroups;
    };

    class BuildPrerequisities
    {
        public:
        std::shared_ptr<DNSDeps> dnsDeps = std::make_shared<DNSDeps>();
        std::shared_ptr<HijackDeps> hijackDeps = std::make_shared<HijackDeps>();
        std::shared_ptr<TunDeps> tunDeps = std::make_shared<TunDeps>();
        std::shared_ptr<RoutingDeps> routingDeps = std::make_shared<RoutingDeps>();
    };

    // One per built chain (main chain + each route outbound group). watchTag is
    // the sing-box outbound tag whose stats represent total bytes for the chain
    // — it's the matched outbound of a routing rule. For chains that re-enter
    // sing-box after an xray hop (e.g. [sing,xray,sing]) there are two such
    // outbounds; we pick the last one in build order so we read traffic at the
    // egress side. profiles is every user-visible hop to credit with the bytes,
    // synthetic socks bridges excluded.
    struct TrafficChainGroup {
        QString watchTag;
        QList<std::shared_ptr<Profile>> profiles;
    };

    // An auto-selector group present in the built config. sing-box's clash
    // tracker resolves a group down to the member that actually carried the
    // connection (see NewTCPTracker), so bytes are never attributed to the
    // group tag — traffic must be watched per member. That is why members get
    // their own TrafficChainGroup entries, and it is also what gives real
    // per-server attribution inside the group.
    struct AutoSelectorBuildInfo {
        QString groupTag;
        std::shared_ptr<Profile> profile;
        // Member outbound tag -> the profile behind it, in ranked order.
        QList<QPair<QString, std::shared_ptr<Profile>>> members;
    };

    class BuildConfigResult {
    public:
        QString error;
        QJsonObject coreConfig;
        QString tunIPv4CIDR;
        bool isXrayNeeded = false;
        QJsonObject xrayConfig;
        std::shared_ptr<ExtraCoreData> extraCoreData = std::make_shared<ExtraCoreData>();

        QList<TrafficChainGroup> chainGroups;
        QList<AutoSelectorBuildInfo> autoSelectors;
    };

    struct coreBridgeConfig {
        bool needed = false;
        int port = -1;
        QString auth;
        // Loopback host (127.x.y.z) used as both listen and dial address for
        // this bridge. Randomizing per-bridge spreads ephemeral source-port
        // allocation across (dst_ip, dst_port) buckets, so a single bridge
        // under load doesn't starve every other bridge of source ports.
        QString host = "127.0.0.1";
    };

    class BuildSingBoxConfigContext
    {
        public:
        bool forTest = false;
        bool forExport = false;
        bool tunEnabled = false;
        bool isResolvedUsed = false;
        bool singToXrayTransitioned = false;
        bool xrayToSingTransitioned = false;
        bool proxyUsesXray = false;
        std::shared_ptr<Profile> ent = std::make_shared<Profile>(nullptr, nullptr);
        std::shared_ptr<BuildPrerequisities> buildPrerequisities = std::make_shared<BuildPrerequisities>();
        osType os;
        // Pre-resolved WARP device name (from the 2s poll cache). Empty means
        // do not pin default_interface. Never filled by probing here — that
        // blocks the UI event loop (see WarpProcess::Status comments).
        QString warpUnderlayInterface;

        QString error;
        QStringList warnings;
        QJsonArray outbounds;
        QJsonArray endpoints;
        QJsonArray xrayOutbounds;
        QList<QString> xrayIngressTags;
        QList<QString> singIngressTags;
        QList<coreBridgeConfig> singToXrayBridges;
        QList<coreBridgeConfig> xrayToSingBridges;
        std::shared_ptr<BuildConfigResult> buildConfigResult = std::make_shared<BuildConfigResult>();
    };

    inline QString get_jsdelivr_link(QString link)
    {
        if(Configs::dataManager->settingsRepo->ruleset_mirror == Mirrors::GITHUB)
            return link;
        if(auto url = QUrl(link); url.isValid() && url.host() == "raw.githubusercontent.com")
        {
            QStringList list = url.path().split('/');
            QString result;
            switch(Configs::dataManager->settingsRepo->ruleset_mirror) {
            case Mirrors::GCORE: result = "https://gcore.jsdelivr.net/gh"; break;
            case Mirrors::QUANTIL: result = "https://quantil.jsdelivr.net/gh"; break;
            case Mirrors::FASTLY: result = "https://fastly.jsdelivr.net/gh"; break;
            case Mirrors::CDN: result = "https://cdn.jsdelivr.net/gh"; break;
            default: result = "https://testingcf.jsdelivr.net/gh";
            }

            int index = 0;
            foreach(QString item, list)
            {
                if(!item.isEmpty())
                {
                    if(index == 2)
                        result += "@" + item;
                    else
                        result += "/" + item;
                    index++;
                }
            }
            return result;
        }
        return link;
    }

    // Prefer jsDelivr; if mirror is GITHUB keep raw unless forceMirror.
    inline QString adblockRulesetUrl() {
        const QString gh = QStringLiteral(
            "https://raw.githubusercontent.com/217heidai/adblockfilters/main/rules/adblocksingbox.srs");
        const QString mirrored = get_jsdelivr_link(gh);
        if (mirrored != gh) return mirrored;
        return QStringLiteral(
            "https://testingcf.jsdelivr.net/gh/217heidai/adblockfilters@main/rules/adblocksingbox.srs");
    }

    // Legacy synthetic ID retained for traffic history / old data only.
    constexpr int warpProfileID = -2408;

    void CalculatePrerequisities(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    void buildLogSections(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    void buildDNSSection(std::shared_ptr<BuildSingBoxConfigContext> &ctx, bool useDnsObj = true);

    void buildNTPSection(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    void buildCertificateSection(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    void buildInboundSection(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    void buildOutboundsSection(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    void buildRouteSection(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    void buildExperimentalSection(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    void buildXrayConfig(std::shared_ptr<BuildSingBoxConfigContext> &ctx);

    std::shared_ptr<BuildConfigResult> BuildSingBoxConfig(const std::shared_ptr<Profile> &ent, bool forExport = false,
                                                          const QString &warpUnderlayInterface = {});

    class BuildTestConfigResult {
    public:
        QString error;
        QMap<int, QString> fullConfigs;
        // Opaque, standalone Xray full configs (one entry per xray-full profile).
        // Their socks outbounds are folded into `coreConfig` and their tags into
        // `outboundTags` / `tag2entID`, so all xray-full profiles share the single
        // test box; each entry here still becomes its own Xray instance on the Go
        // side (TestReq.xray_full_configs). See BuildTestConfig.
        QStringList xrayFullConfigs;
        QMap<QString, int> tag2entID;
        QJsonObject coreConfig;
        QJsonObject xrayConfig;
        bool isXrayNeeded = false;
        QStringList outboundTags;
    };

    bool IsValid(const std::shared_ptr<Profile> &ent);

    std::shared_ptr<BuildTestConfigResult> BuildTestConfig(const QList<std::shared_ptr<Profile> > &profiles);
}
