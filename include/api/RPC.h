#pragma once

#ifndef Q_MOC_RUN
#include <core/server/gen/libcore.pb.h>
#endif
#include <QString>
#include <functional>
#include <optional>
#include <string>

class QLocalSocket;

namespace API {
    struct WgKeyPairResult {
        QString privateKey;
        QString publicKey;
        QString error;

        [[nodiscard]] bool ok() const {
            return error.isEmpty() && !privateKey.isEmpty() && !publicKey.isEmpty();
        }
    };

    namespace detail {
        WgKeyPairResult NormalizeWgKeyPairResult(
            int status,
            bool decoded,
            const std::optional<std::string>& privateKey,
            const std::optional<std::string>& publicKey,
            const std::optional<std::string>& error);
    }

    class Client {
    public:
        Client();

        ~Client();

        // Adopt a freshly connected socket, replacing any previous
        // connection. The Client itself is long-lived and never recreated.
        void Reconnect(QLocalSocket *socket);

        // QString returns is error string

        [[nodiscard]] QString Start(bool *rpcOK, const libcore::LoadConfigReq &request,
                      const std::function<bool()> &canceled = {});

        [[nodiscard]] QString Stop(bool *rpcOK);

        [[nodiscard]] libcore::QueryStatsResp QueryStats();

        // coreError (optional): on RPC failure, receives the core's error message
        // so callers can react to it (e.g. missing Xray geo assets) rather than
        // silently dropping the failed test.
        [[nodiscard]] libcore::TestResp Test(bool *rpcOK, const libcore::TestReq &request, QString *coreError = nullptr);

        void StopTests(bool *rpcOK);

        [[nodiscard]] libcore::QueryURLTestResponse QueryURLTest(bool *rpcOK);

        [[nodiscard]] libcore::IPTestResp IPTest(bool *rpcOK, const libcore::IPTestRequest &request, QString *coreError = nullptr);

        [[nodiscard]] libcore::QueryIPTestResponse QueryIPTest(bool *rpcOK);

        [[nodiscard]] QString SetSystemDNS(bool *rpcOK, bool clear) const;

        [[nodiscard]] libcore::QueryConnectionsResp QueryConnections() const;

        [[nodiscard]] libcore::CoreStateResponse QueryState(bool *rpcOK) const;

        // isXray selects the validating core: false (default) validates a
        // sing-box config, true validates an Xray-format config.
        [[nodiscard]] QString CheckConfig(bool *rpcOK, const QString& config, bool isXray = false) const;

        [[nodiscard]] bool IsPrivileged(bool *rpcOK) const;

        [[nodiscard]] libcore::SpeedTestResponse SpeedTest(bool *rpcOK, const libcore::SpeedTestRequest &request, QString *coreError = nullptr);

        [[nodiscard]] libcore::QuerySpeedTestResponse QueryCurrentSpeedTests(bool *rpcOK);

        [[nodiscard]] libcore::QueryCountryTestResponse QueryCountryTestResults(bool *rpcOK);

        [[nodiscard]] WgKeyPairResult GenWgKeyPair();

        // Empty name = the OS has no default route. A local, censorship-proof
        // way to tell "my network died" from "the servers died".
        [[nodiscard]] libcore::GetDefaultInterfaceResponse GetDefaultInterface(bool *rpcOK) const;

        // Idempotent snapshot of every running auto-selector group: it clears no
        // core-side counters, so polling it alongside QueryStats is safe.
        [[nodiscard]] libcore::QueryAutoSelectorsResponse QueryAutoSelectors(bool *rpcOK) const;

        // action: "recheck" forces a full sweep now, "select" pins the group to
        // member. An empty tag targets every auto-selector group.
        QString AutoSelectorAction(bool *rpcOK, const QString &tag, const QString &action,
                                   const QString &member = {}) const;

    private:
        class LocalSocketChannel;
        std::unique_ptr<LocalSocketChannel> channel;
    };

    inline Client *defaultClient;
} // namespace API
