#include "QvProxyConfigurator.hpp"

#ifdef Q_OS_WIN
//
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
//
#include <wininet.h>
#include <ras.h>
#include <raserror.h>
#include <vector>
#endif

#include <QStandardPaths>
#include <QProcess>

#include "3rdparty/qv2ray/wrapper.hpp"
#include "include/global/Configs.hpp"

#define QV_MODULE_NAME "SystemProxy"

#define QSTRN(num) QString::number(num)

namespace Qv2ray::components::proxy {

    using ProcessArgument = QPair<QString, QStringList>;
#ifdef Q_OS_MACOS
    // networksetup can hang if System Configuration is wedged; never block UI forever.
    constexpr int kNetworkSetupTimeoutMs = 5000;

    bool runNetworkSetup(const QStringList &args) {
        QProcess p;
        p.start(QStringLiteral("/usr/sbin/networksetup"), args);
        if (!p.waitForStarted(kNetworkSetupTimeoutMs)) {
            LOG("networksetup failed to start: " + p.errorString() + " args=" + args.join(" "));
            return false;
        }
        if (!p.waitForFinished(kNetworkSetupTimeoutMs)) {
            p.kill();
            p.waitForFinished(1000);
            LOG("networksetup timed out: args=" + args.join(" "));
            return false;
        }
        if (p.exitStatus() != QProcess::NormalExit || p.exitCode() != 0) {
            const auto err = QString::fromLocal8Bit(p.readAllStandardError()).trimmed();
            LOG("networksetup exit=" + QSTRN(p.exitCode()) + " args=" + args.join(" ")
                + (err.isEmpty() ? QString() : (" err=" + err)));
            return false;
        }
        return true;
    }

    QStringList macOSgetNetworkServices() {
        QProcess p;
        p.start(QStringLiteral("/usr/sbin/networksetup"), {QStringLiteral("-listallnetworkservices")});
        if (!p.waitForStarted(kNetworkSetupTimeoutMs) || !p.waitForFinished(kNetworkSetupTimeoutMs)) {
            if (p.state() != QProcess::NotRunning) {
                p.kill();
                p.waitForFinished(1000);
            }
            LOG("networksetup -listallnetworkservices failed: " + p.errorString());
            return {};
        }
        if (p.exitStatus() != QProcess::NormalExit || p.exitCode() != 0) {
            LOG("networksetup -listallnetworkservices exit=" + QSTRN(p.exitCode()));
            return {};
        }
        auto lines = SplitLines(p.readAllStandardOutput());
        QStringList result;

        // Start from 1 since first line is unneeded.
        for (auto i = 1; i < lines.count(); i++) {
            // * means disabled.
            if (!lines[i].contains("*")) {
                result << lines[i];
            }
        }

        LOG("Found " + QSTRN(result.size()) + " network services: " + result.join(";"));
        return result;
    }
#endif
#ifdef Q_OS_WIN
#define NO_CONST(expr) const_cast<wchar_t *>(expr)
    // static auto DEFAULT_CONNECTION_NAME =
    // NO_CONST(L"DefaultConnectionSettings");
    ///
    /// INTERNAL FUNCTION
    bool __QueryProxyOptions() {
        INTERNET_PER_CONN_OPTION_LIST List{};
        INTERNET_PER_CONN_OPTION Option[5]{};
        unsigned long nSize = sizeof(INTERNET_PER_CONN_OPTION_LIST);
        Option[0].dwOption = INTERNET_PER_CONN_AUTOCONFIG_URL;
        Option[1].dwOption = INTERNET_PER_CONN_AUTODISCOVERY_FLAGS;
        Option[2].dwOption = INTERNET_PER_CONN_FLAGS;
        Option[3].dwOption = INTERNET_PER_CONN_PROXY_BYPASS;
        Option[4].dwOption = INTERNET_PER_CONN_PROXY_SERVER;
        List.dwSize = sizeof(INTERNET_PER_CONN_OPTION_LIST);
        List.pszConnection = nullptr;
        List.dwOptionCount = 5;
        List.dwOptionError = 0;
        List.pOptions = Option;

        const bool success =
            InternetQueryOption(nullptr, INTERNET_OPTION_PER_CONNECTION_OPTION, &List, &nSize) != FALSE;
        const DWORD error = success ? ERROR_SUCCESS : GetLastError();

        if (!success) {
            LOG("InternetQueryOption failed, GLE=" + QSTRN(error));
        } else {
            LOG("System default proxy info:");
            if (Option[0].Value.pszValue != nullptr) {
                LOG(QString::fromWCharArray(Option[0].Value.pszValue));
            }
            if ((Option[2].Value.dwValue & PROXY_TYPE_AUTO_PROXY_URL) == PROXY_TYPE_AUTO_PROXY_URL) {
                LOG("PROXY_TYPE_AUTO_PROXY_URL");
            }
            if ((Option[2].Value.dwValue & PROXY_TYPE_AUTO_DETECT) == PROXY_TYPE_AUTO_DETECT) {
                LOG("PROXY_TYPE_AUTO_DETECT");
            }
            if ((Option[2].Value.dwValue & PROXY_TYPE_DIRECT) == PROXY_TYPE_DIRECT) {
                LOG("PROXY_TYPE_DIRECT");
            }
            if ((Option[2].Value.dwValue & PROXY_TYPE_PROXY) == PROXY_TYPE_PROXY) {
                LOG("PROXY_TYPE_PROXY");
            }
            if (Option[4].Value.pszValue != nullptr) {
                LOG(QString::fromStdWString(Option[4].Value.pszValue));
            }
        }

        // Always free any strings WinINet may have allocated, including partial
        // results on failure. Value-init keeps untouched slots null.
        if (Option[0].Value.pszValue != nullptr) {
            GlobalFree(Option[0].Value.pszValue);
        }
        if (Option[3].Value.pszValue != nullptr) {
            GlobalFree(Option[3].Value.pszValue);
        }
        if (Option[4].Value.pszValue != nullptr) {
            GlobalFree(Option[4].Value.pszValue);
        }
        return success;
    }
    bool __SetProxyOptions(LPWSTR proxy_full_addr, bool isPAC) {
        INTERNET_PER_CONN_OPTION_LIST list;
        DWORD dwBufSize = sizeof(list);
        // Fill the list structure.
        list.dwSize = sizeof(list);
        // NULL == LAN, otherwise connectoid name.
        list.pszConnection = nullptr;

        if (nullptr == proxy_full_addr) {
            LOG("Clearing system proxy");
            //
            list.dwOptionCount = 1;
            list.pOptions = new INTERNET_PER_CONN_OPTION[1];

            // Ensure that the memory was allocated.
            if (nullptr == list.pOptions) {
                // Return if the memory wasn't allocated.
                return false;
            }

            // Set flags.
            list.pOptions[0].dwOption = INTERNET_PER_CONN_FLAGS;
            list.pOptions[0].Value.dwValue = PROXY_TYPE_DIRECT;
        } else if (isPAC) {
            LOG("Setting system proxy for PAC");
            //
            list.dwOptionCount = 2;
            list.pOptions = new INTERNET_PER_CONN_OPTION[2];

            if (nullptr == list.pOptions) {
                return false;
            }

            // Set flags.
            list.pOptions[0].dwOption = INTERNET_PER_CONN_FLAGS;
            list.pOptions[0].Value.dwValue = PROXY_TYPE_DIRECT | PROXY_TYPE_AUTO_PROXY_URL;
            // Set proxy name.
            list.pOptions[1].dwOption = INTERNET_PER_CONN_AUTOCONFIG_URL;
            list.pOptions[1].Value.pszValue = proxy_full_addr;
        } else {
            LOG("Setting system proxy for Global Proxy");
            //
            list.dwOptionCount = 2;
            list.pOptions = new INTERNET_PER_CONN_OPTION[2];

            if (nullptr == list.pOptions) {
                return false;
            }

            // Set flags.
            list.pOptions[0].dwOption = INTERNET_PER_CONN_FLAGS;
            list.pOptions[0].Value.dwValue = PROXY_TYPE_DIRECT | PROXY_TYPE_PROXY;
            // Set proxy name.
            list.pOptions[1].dwOption = INTERNET_PER_CONN_PROXY_SERVER;
            list.pOptions[1].Value.pszValue = proxy_full_addr;
            // Set proxy override.
            // list.pOptions[2].dwOption = INTERNET_PER_CONN_PROXY_BYPASS;
            // auto localhost = L"localhost";
            // list.pOptions[2].Value.pszValue = NO_CONST(localhost);
        }

        // Set proxy for LAN.
        if (!InternetSetOption(nullptr, INTERNET_OPTION_PER_CONNECTION_OPTION, &list, dwBufSize)) {
            LOG("InternetSetOption failed for LAN, GLE=" + QSTRN(GetLastError()));
        }

        RASENTRYNAME entry;
        entry.dwSize = sizeof(entry);
        std::vector<RASENTRYNAME> entries;
        DWORD size = sizeof(entry), count;
        LPRASENTRYNAME entryAddr = &entry;
        auto ret = RasEnumEntries(nullptr, nullptr, entryAddr, &size, &count);
        if (ERROR_BUFFER_TOO_SMALL == ret) {
            entries.resize(count);
            entries[0].dwSize = sizeof(RASENTRYNAME);
            entryAddr = entries.data();
            ret = RasEnumEntries(nullptr, nullptr, entryAddr, &size, &count);
        }
        if (ERROR_SUCCESS != ret) {
            LOG("Failed to list entry names");
            return false;
        }

        // Set proxy for each connectoid.
        for (DWORD i = 0; i < count; ++i) {
            list.pszConnection = entryAddr[i].szEntryName;
            if (!InternetSetOption(nullptr, INTERNET_OPTION_PER_CONNECTION_OPTION, &list, dwBufSize)) {
                LOG("InternetSetOption failed for connectoid " + QString::fromWCharArray(list.pszConnection) + ", GLE=" + QSTRN(GetLastError()));
            }
        }

        delete[] list.pOptions;
        InternetSetOption(nullptr, INTERNET_OPTION_SETTINGS_CHANGED, nullptr, 0);
        InternetSetOption(nullptr, INTERNET_OPTION_REFRESH, nullptr, 0);
        return true;
    }
#endif

    bool SetSystemProxy(int httpPort, int socksPort, QString scheme) {
        const QString &address = "127.0.0.1";
        bool hasHTTP = (httpPort > 0 && httpPort < 65536);
        bool hasSOCKS = (socksPort > 0 && socksPort < 65536);

#ifdef Q_OS_WIN
        if (!hasHTTP) {
            LOG("Nothing?");
            return false;
        } else {
            LOG("Qv2ray will set system proxy to use HTTP");
        }
#else
        if (!hasHTTP && !hasSOCKS) {
            LOG("Nothing?");
            return false;
        }

        if (hasHTTP) {
            LOG("Qv2ray will set system proxy to use HTTP");
        }

        if (hasSOCKS) {
            LOG("Qv2ray will set system proxy to use SOCKS");
        }
#endif

#ifdef Q_OS_WIN
        if (scheme == "http") scheme = "http://{ip}:{port}";
        else if (scheme == "socks") scheme = "socks={ip}:{port}";
        scheme = scheme.replace("{ip}", address)
                  .replace("{port}", Int2String(socksPort));
        //
        LOG("Windows proxy string: " + scheme);
        auto proxyStrW = new WCHAR[scheme.length() + 1];
        wcscpy(proxyStrW, scheme.toStdWString().c_str());
        //
        __QueryProxyOptions();

        const bool ok = __SetProxyOptions(proxyStrW, false);
        if (!ok) {
            LOG("Failed to set proxy.");
        }

        __QueryProxyOptions();
        return ok;
#elif defined(Q_OS_LINUX)
        QList<ProcessArgument> actions;
        //
        bool isKDE = qEnvironmentVariable("XDG_CURRENT_DESKTOP") == "KDE" ||
                     qEnvironmentVariable("XDG_CURRENT_DESKTOP") == "Trinity";
        const auto configPath = QStandardPaths::writableLocation(QStandardPaths::ConfigLocation);
        QString kwriteconfigCmd = qEnvironmentVariable("KDE_SESSION_VERSION") == "5" ? "kwriteconfig5" : qEnvironmentVariable("KDE_SESSION_VERSION") == "6" ? "kwriteconfig6" : "kwriteconfig";

        //
        // Configure HTTP Proxies for HTTP, FTP and HTTPS
        if (hasHTTP) {
            // iterate over protocols...
            for (const auto &protocol: QStringList{"http", "ftp", "https"}) {
                // for GNOME:
                {
                    actions << ProcessArgument{"gsettings",
                                               {"set", "org.gnome.system.proxy." + protocol, "host", address}};
                    actions << ProcessArgument{"gsettings",
                                               {"set", "org.gnome.system.proxy." + protocol, "port", QSTRN(httpPort)}};
                }

                // for KDE:
                if (isKDE) {
                    actions << ProcessArgument{kwriteconfigCmd,
                                               {"--file", configPath + "/kioslaverc", //
                                                "--group", "Proxy Settings",          //
                                                "--key", protocol + "Proxy",          //
                                                "http://" + address + " " + QSTRN(httpPort)}};
                }
            }
        }

        // Configure SOCKS5 Proxies
        if (hasSOCKS) {
            // for GNOME:
            {
                actions << ProcessArgument{"gsettings", {"set", "org.gnome.system.proxy.socks", "host", address}};
                actions << ProcessArgument{"gsettings",
                                           {"set", "org.gnome.system.proxy.socks", "port", QSTRN(socksPort)}};

                // for KDE:
                if (isKDE) {
                    actions << ProcessArgument{kwriteconfigCmd,
                                               {"--file", configPath + "/kioslaverc", //
                                                "--group", "Proxy Settings",          //
                                                "--key", "socksProxy",                //
                                                "socks://" + address + " " + QSTRN(socksPort)}};
                }
            }
        }
        // Setting Proxy Mode to Manual
        {
            // for GNOME:
            {
                actions << ProcessArgument{"gsettings", {"set", "org.gnome.system.proxy", "mode", "manual"}};
            }

            // for KDE:
            if (isKDE) {
                actions << ProcessArgument{kwriteconfigCmd,
                                           {"--file", configPath + "/kioslaverc", //
                                            "--group", "Proxy Settings",          //
                                            "--key", "ProxyType", "1"}};
            }
        }

        // Notify kioslaves to reload system proxy configuration.
        if (isKDE) {
            actions << ProcessArgument{"dbus-send",
                                       {"--type=signal", "/KIO/Scheduler",                 //
                                        "org.kde.KIO.Scheduler.reparseSlaveConfiguration", //
                                        "string:''"}};
        }
        // Execute them all!
        //
        // note: do not use std::all_of / any_of / none_of,
        // because those are short-circuit and cannot guarantee atomicity.
        QList<bool> results;
        for (const auto &action: actions) {
            // execute and get the code
            const auto returnCode = QProcess::execute(action.first, action.second);
            // print out the commands and result codes
            DEBUG(QString("[%1] Program: %2, Args: %3").arg(returnCode).arg(action.first).arg(action.second.join(";")));
            // give the code back
            results << (returnCode == QProcess::NormalExit);
        }

        if (results.count(true) != actions.size()) {
            LOG("Something wrong when setting proxies.");
            return false;
        }
        return true;
#else
        const auto services = macOSgetNetworkServices();
        if (services.isEmpty()) {
            LOG("No network services to configure system proxy");
            return false;
        }
        bool ok = true;
        for (const auto &service: services) {
            LOG("Setting proxy for interface: " + service);
            if (hasHTTP) {
                ok = runNetworkSetup({"-setwebproxy", service, address, QSTRN(httpPort)}) && ok;
                ok = runNetworkSetup({"-setsecurewebproxy", service, address, QSTRN(httpPort)}) && ok;
                ok = runNetworkSetup({"-setwebproxystate", service, "on"}) && ok;
                ok = runNetworkSetup({"-setsecurewebproxystate", service, "on"}) && ok;
            }

            if (hasSOCKS) {
                ok = runNetworkSetup({"-setsocksfirewallproxy", service, address, QSTRN(socksPort)}) && ok;
                ok = runNetworkSetup({"-setsocksfirewallproxystate", service, "on"}) && ok;
            }
        }
        return ok;

#endif
    }

    bool ClearSystemProxy() {
        LOG("Clearing System Proxy");

#ifdef Q_OS_WIN
        if (!__SetProxyOptions(nullptr, false)) {
            LOG("Failed to clear proxy.");
            return false;
        }
        return true;
#elif defined(Q_OS_LINUX)
        QList<ProcessArgument> actions;
        const bool isKDE = qEnvironmentVariable("XDG_CURRENT_DESKTOP") == "KDE" ||
                           qEnvironmentVariable("XDG_CURRENT_DESKTOP") == "Trinity";
        const auto configRoot = QStandardPaths::writableLocation(QStandardPaths::ConfigLocation);

        // Setting System Proxy Mode to: None
        {
            // for GNOME:
            {
                actions << ProcessArgument{"gsettings", {"set", "org.gnome.system.proxy", "mode", "none"}};
            }

            // for KDE:
            if (isKDE) {
                actions << ProcessArgument{qEnvironmentVariable("KDE_SESSION_VERSION") == "5" ? "kwriteconfig5" : qEnvironmentVariable("KDE_SESSION_VERSION") == "6" ? "kwriteconfig6" : "kwriteconfig",
                                           {"--file", configRoot + "/kioslaverc", //
                                            "--group", "Proxy Settings",          //
                                            "--key", "ProxyType", "0"}};
            }
        }

        // Notify kioslaves to reload system proxy configuration.
        if (isKDE) {
            actions << ProcessArgument{"dbus-send",
                                       {"--type=signal", "/KIO/Scheduler",                 //
                                        "org.kde.KIO.Scheduler.reparseSlaveConfiguration", //
                                        "string:''"}};
        }

        // Execute the Actions
        bool ok = true;
        for (const auto &action: actions) {
            // execute and get the code
            const auto returnCode = QProcess::execute(action.first, action.second);
            // print out the commands and result codes
            DEBUG(QString("[%1] Program: %2, Args: %3").arg(returnCode).arg(action.first).arg(action.second.join(";")));
            if (returnCode != QProcess::NormalExit) ok = false;
        }
        return ok;

#else
        const auto services = macOSgetNetworkServices();
        if (services.isEmpty()) {
            LOG("No network services to clear system proxy");
            return false;
        }
        bool ok = true;
        for (const auto &service: services) {
            LOG("Clearing proxy for interface: " + service);
            ok = runNetworkSetup({"-setautoproxystate", service, "off"}) && ok;
            ok = runNetworkSetup({"-setwebproxystate", service, "off"}) && ok;
            ok = runNetworkSetup({"-setsecurewebproxystate", service, "off"}) && ok;
            ok = runNetworkSetup({"-setsocksfirewallproxystate", service, "off"}) && ok;
        }
        return ok;

#endif
    }
} // namespace Qv2ray::components::proxy
