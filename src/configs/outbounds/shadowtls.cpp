#include "include/configs/outbounds/shadowtls.h"
#include "include/configs/ProtocolDefaults.h"

#include <QUrlQuery>
#include <include/global/Utils.hpp>

#include "include/configs/common/utils.h"

namespace Configs {
    bool shadowtls::ParseFromLink(const QString& link)
    {
        auto url = QUrl(link);
        if (!url.isValid()) return false;
        auto query = QUrlQuery(url.query());

        outbound::ParseFromLink(link);
        if (query.hasQueryItem("version"))
        {
            version = parseIntOr(query.queryItemValue("version"), version);
        }
        password = url.password();
        
        tls->ParseFromLink(link);
        tls->enabled = true; // ShadowTLS always uses tls
        
        if (server_port == 0) server_port = DEFAULT_TLS_PORT;

        return !server.isEmpty();
    }

    bool shadowtls::ParseFromJson(const QJsonObject& object)
    {
        if (object.isEmpty() || object["type"].toString() != "shadowtls") return false;
        outbound::ParseFromJson(object);
        if (object.contains("version")) version = object["version"].toInt();
        if (object.contains("password")) password = object["password"].toString();
        if (object.contains("tls")) tls->ParseFromJson(object["tls"].toObject());
        return true;
    }

    QString shadowtls::ExportToLink() const
    {
        QUrl url;
        QUrlQuery query;
        url.setScheme("shadowtls");
        if (version > 1 && !password.isEmpty()) url.setPassword(password);
        url.setHost(server);
        url.setPort(server_port);
        if (!name.isEmpty()) url.setFragment(name);

        query.addQueryItem("version", Int2String(version));

        mergeUrlQuery(query, tls->ExportToLink());
        mergeUrlQuery(query, outbound::ExportToLink());
        
        if (!query.isEmpty()) url.setQuery(query);
        return url.toString(QUrl::FullyEncoded);
    }

    QJsonObject shadowtls::ExportToJson() const
    {
        QJsonObject object;
        object["type"] = "shadowtls";
        mergeJsonObjects(object, outbound::ExportToJson());
        object["version"] = version;
        if (version > 1 && !password.isEmpty()) object["password"] = password;
        if (tls->enabled) object["tls"] = tls->ExportToJson();
        return object;
    }

    QJsonObject shadowtls::ExportIdentity() const
    {
        auto object = outbound::ExportIdentity();
        object["version"] = version;
        return object;
    }

    BuildResult shadowtls::Build() const
    {
        QJsonObject object;
        object["type"] = "shadowtls";
        mergeJsonObjects(object, outbound::Build().object);
        object["version"] = version;
        if (version > 1 && !password.isEmpty()) object["password"] = password;
        if (tls->enabled) object["tls"] = tls->Build().object;
        return {object, ""};
    }

    QString shadowtls::DisplayType() const
    {
        return "ShadowTLS";
    }
}
