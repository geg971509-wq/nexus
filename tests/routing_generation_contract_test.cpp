#include "include/configs/generate.h"

#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QString>
#include <QStringList>

#include <cstdlib>
#include <iostream>
#include <initializer_list>

#ifndef THRONE_SOURCE_DIR
#error THRONE_SOURCE_DIR must be defined
#endif

using Configs::HijackDeps;

namespace {
HijackDeps parseHijackRules(const QStringList &rules) {
    HijackDeps deps;
    for (const auto &rule : rules) {
        if (rule.startsWith("ruleset:")) deps.hijackGeoAssets << rule.mid(8);
        if (rule.startsWith("domain:")) deps.hijackDomains << rule.mid(7);
        if (rule.startsWith("suffix:")) deps.hijackDomainSuffix << rule.mid(7);
        if (rule.startsWith("regex:")) deps.hijackDomainRegex << rule.mid(6);
    }
    return deps;
}

QString readSource(const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) return {};
    return QString::fromUtf8(file.readAll());
}

QString section(const QString &source, const QString &beginMarker,
                const QString &endMarker) {
    const auto begin = source.indexOf(beginMarker);
    const auto end = source.indexOf(endMarker, begin + beginMarker.size());
    if (begin < 0 || end < 0) return {};
    return source.mid(begin, end - begin);
}

QString activeCode(QString source) {
    static const QRegularExpression comments(
        QStringLiteral(R"(/\*[\s\S]*?\*/|//[^\n]*)"));
    source.remove(comments);
    return source;
}

QString compact(QString source) {
    source.remove(QRegularExpression(QStringLiteral("\\s+")));
    return source;
}

bool ordered(std::initializer_list<qsizetype> positions) {
    qsizetype previous = -1;
    for (const auto position : positions) {
        if (position < 0 || position <= previous) return false;
        previous = position;
    }
    return true;
}
}

int main(int argc, char **argv) {
    const QString sourcePath = argc > 1
        ? QString::fromLocal8Bit(argv[1])
        : QStringLiteral(THRONE_SOURCE_DIR "/src/configs/generate.cpp");
    const QString source = readSource(sourcePath);
    const QString prerequisites = activeCode(section(
        source,
        QStringLiteral("    void CalculatePrerequisities("),
        QStringLiteral("\n    void buildLogSections(")));
    const QString dns = activeCode(section(
        source,
        QStringLiteral("    void buildDNSSection("),
        QStringLiteral("\n    void buildInboundSection(")));
    const QString build = activeCode(section(
        source,
        QStringLiteral("    std::shared_ptr<BuildConfigResult> BuildSingBoxConfig("),
        QStringLiteral("\n    bool IsValid(")));

    const QString prerequisiteCode = compact(prerequisites);
    const QString dnsCode = compact(dns);
    const QString buildCode = compact(build);
    QJsonObject checks;
    const auto check = [&checks](const char *name, bool pass) {
        checks.insert(QString::fromLatin1(name), pass);
    };

    const auto emptyDeps = parseHijackRules({});
    const auto ignoredDeps = parseHijackRules(
        {"example.com", "ruleset", "notaprefix:x", ""});
    const auto parsedDeps = parseHijackRules(
        {"ruleset:geosite-ads", "domain:a.com", "suffix:.b.com", "regex:^c\\."});
    check("hijack_empty", emptyDeps.noConditions());
    check("hijack_unrecognised", ignoredDeps.noConditions());
    check("hijack_recognised",
          !parseHijackRules({"ruleset:geosite-ads"}).noConditions() &&
          !parseHijackRules({"domain:ads.example.com"}).noConditions() &&
          !parseHijackRules({"suffix:.ads.example.com"}).noConditions() &&
          !parseHijackRules({"regex:^ads\\."}).noConditions());
    check("hijack_prefix_values",
          !parsedDeps.noConditions() &&
          parsedDeps.hijackGeoAssets == QJsonArray{"geosite-ads"} &&
          parsedDeps.hijackDomains == QJsonArray{"a.com"} &&
          parsedDeps.hijackDomainSuffix == QJsonArray{".b.com"} &&
          parsedDeps.hijackDomainRegex == QJsonArray{"^c\\."});

    check("source_sections",
          !prerequisites.isEmpty() && !dns.isEmpty() && !build.isEmpty());
    check("uses_xray_core",
          prerequisiteCode.contains(
              "returnp->outbound!=nullptr&&(p->outbound->IsXray()||"
              "p->outbound->IsXrayFullConfig());"));

    const QString routingChain = section(
        prerequisiteCode,
        QStringLiteral("if(neededEnt->type==\"chain\"){"),
        QStringLiteral("preReqs->routingDeps->outboundMap[item]=\"route-\"+"));
    check("routing_chain_hop_assignment",
          routingChain.contains(
              "if(usesXrayCore(hopEnt))ctx->proxyUsesXray=true;") &&
          ordered({routingChain.indexOf("Chainhopsinroutingprofilecannotuse"),
                   routingChain.indexOf("usesXrayCore(hopEnt)"),
                   routingChain.indexOf("getEntDomains({hopID}")}));

    const QString routingSingle = section(
        prerequisiteCode,
        QStringLiteral("suffix+=chain->list.size();}else{"),
        QStringLiteral("preReqs->routingDeps->routeOutboundGroups<<"
                       "RoutingDeps::RouteOutboundGroup{QList<int>{item},nullptr};"));
    check("routing_single_assignment",
          routingSingle.contains(
              "if(usesXrayCore(neededEnt))ctx->proxyUsesXray=true;") &&
          ordered({routingSingle.indexOf("usesXrayCore(neededEnt)"),
                   routingSingle.indexOf("getEntDomains({neededEnt->id}")}));

    const QString group = section(
        prerequisiteCode,
        QStringLiteral("if(autogroup=Configs::dataManager->groupsRepo->GetGroup("),
        QStringLiteral("if(s->enable_dns_server){"));
    check("group_front_landing_assignment",
          group.contains("front_proxy_id") &&
          group.contains("landing_proxy_id") &&
          group.contains("for(constautoid:groupEnts)") &&
          group.contains("profilesRepo->GetProfile(id)") &&
          group.contains("ent!=nullptr&&usesXrayCore(ent)") &&
          group.contains("ctx->proxyUsesXray=true;") &&
          ordered({group.indexOf("front_proxy_id"),
                   group.indexOf("landing_proxy_id"),
                   group.indexOf("for(constautoid:groupEnts)"),
                   group.indexOf("getEntDomains(groupEnts")}));

    const QString bootstrapRule =
        "if(!ctx->forTest&&ctx->proxyUsesXray){rules+=QJsonObject{"
        "{\"inbound\",QJsonArray{\"dns-in\"}},{\"action\",\"route\"},"
        "{\"strategy\",s->direct_dns_strategy},{\"server\",\"dns-direct\"},};}";
    check("dns_bootstrap_rule",
          dnsCode.count("{\"inbound\",QJsonArray{\"dns-in\"}}") == 1 &&
          dnsCode.contains(bootstrapRule));
    check("dns_hijack_callsite_guard",
          dnsCode.contains("if(s->enable_dns_server&&!ctx->forTest&&"
                           "!hijackDeps->noConditions())"));
    check("dns_rule_order",
          ordered({dnsCode.indexOf("localhost.INA127.0.0.1"),
                   dnsCode.indexOf("{\"rcode\",\"NXDOMAIN\"}"),
                   dnsCode.indexOf("{\"inbound\",QJsonArray{\"dns-in\"}}"),
                   dnsCode.indexOf("extraCoreData->path.isEmpty()"),
                   dnsCode.indexOf("hijackDeps->noConditions()"),
                   dnsCode.indexOf("if(s->fake_dns)")}));
    check("build_order",
          ordered({buildCode.indexOf("CalculatePrerequisities(ctx);"),
                   buildCode.indexOf("buildDNSSection(ctx);"),
                   buildCode.indexOf("buildOutboundsSection(ctx);")}));

    bool pass = !checks.isEmpty();
    for (auto it = checks.cbegin(); it != checks.cend(); ++it) {
        pass = pass && it.value().toBool();
    }
    const QJsonObject result{
        {QStringLiteral("pass"), pass},
        {QStringLiteral("source"), sourcePath},
        {QStringLiteral("checks"), checks},
    };
    std::cout << QJsonDocument(result).toJson(QJsonDocument::Compact).constData();
    return pass ? EXIT_SUCCESS : EXIT_FAILURE;
}
