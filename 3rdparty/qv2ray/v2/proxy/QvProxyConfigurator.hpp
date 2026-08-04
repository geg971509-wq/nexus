#pragma once
#include <QHostAddress>
#include <QObject>
#include <QString>
//
namespace Qv2ray::components::proxy {
    // false = at least one platform step failed (macOS: networksetup exit/timeout).
    bool ClearSystemProxy();
    bool SetSystemProxy(int http_port, int socks_port, QString scheme);
} // namespace Qv2ray::components::proxy

using namespace Qv2ray::components;
using namespace Qv2ray::components::proxy;
