#include "include/api/RPC.h"
#include "include/global/Utils.hpp"

#include <QCoreApplication>
#include <QDataStream>
#include <QDebug>
#include <QElapsedTimer>
#include <QLocalServer>
#include <QLocalSocket>
#include <QUuid>

#include <cstdlib>
#include <functional>
#include <future>
#include <optional>
#include <string>
#include <QThread>

namespace {
    constexpr quint32 kMaxPayloadSize = 16U * 1024U * 1024U;

    struct PeerResult {
        qint64 requestBytes = 0;
        bool disconnected = false;
    };

    QByteArray responseHeader(quint32 requestId, quint8 status, quint32 dataLength) {
        QByteArray frame;
        QDataStream stream(&frame, QIODevice::WriteOnly);
        stream.setByteOrder(QDataStream::LittleEndian);
        stream << requestId << status << dataLength;
        return frame;
    }

    quint32 requestId(const QByteArray& request) {
        quint32 id = 0;
        QDataStream stream(request);
        stream.setByteOrder(QDataStream::LittleEndian);
        stream >> id;
        return id;
    }

    bool waitForRequest(QLocalSocket& socket, QByteArray& request, int timeoutMs) {
        if (socket.bytesAvailable() == 0 && !socket.waitForReadyRead(timeoutMs)) return false;
        request += socket.readAll();
        while (request.size() < static_cast<qsizetype>(sizeof(quint32))) {
            if (!socket.waitForReadyRead(timeoutMs)) return false;
            request += socket.readAll();
        }
        return true;
    }

    using PeerAction = std::function<void(QLocalSocket&, const QByteArray&, PeerResult&)>;
    using ClientAction = std::function<void(API::Client&)>;

    qint64 runExchange(const PeerAction& peerAction, const ClientAction& clientAction,
                       PeerResult& peerResult, int requestTimeoutMs = 2000) {
        const QString serverName = "trpc-" + QUuid::createUuid().toString(QUuid::Id128).first(8);
        std::promise<bool> listeningPromise;
        auto listening = listeningPromise.get_future();

        auto* peer = QThread::create([&] {
            QLocalServer server;
            QLocalServer::removeServer(serverName);
            const bool started = server.listen(serverName);
            listeningPromise.set_value(started);
            if (!started || !server.waitForNewConnection(2000)) return;

            std::unique_ptr<QLocalSocket> socket(server.nextPendingConnection());
            QByteArray request;
            if (waitForRequest(*socket, request, requestTimeoutMs)) {
                peerResult.requestBytes = request.size();
            }
            peerAction(*socket, request, peerResult);
            socket->abort();
            QLocalServer::removeServer(serverName);
        });
        peer->start();

        if (!listening.get()) {
            peer->wait();
            delete peer;
            return -1;
        }

        auto* socket = new QLocalSocket;
        socket->connectToServer(serverName);
        if (!socket->waitForConnected(2000)) {
            delete socket;
            peer->wait();
            delete peer;
            return -1;
        }

        API::Client client;
        client.Reconnect(socket);
        QElapsedTimer timer;
        timer.start();
        clientAction(client);
        const qint64 elapsed = timer.elapsed();
        peer->wait();
        delete peer;
        return elapsed;
    }

    bool expectFailure(int status, bool decoded,
                       const std::optional<std::string>& privateKey,
                       const std::optional<std::string>& publicKey,
                       const std::optional<std::string>& error,
                       const QString& expectedError) {
        const auto result = API::detail::NormalizeWgKeyPairResult(
            status, decoded, privateKey, publicKey, error);
        return !result.ok() && result.error == expectedError;
    }

    bool testSplitFrame() {
        PeerResult peer;
        bool rpcOK = false;
        const auto elapsed = runExchange([](QLocalSocket& socket, const QByteArray& request, PeerResult&) {
            const auto frame = responseHeader(requestId(request), 0, 0);
            socket.write(frame.first(4));
            socket.waitForBytesWritten(1000);
            QThread::msleep(25);
            socket.write(frame.sliced(4));
            socket.waitForBytesWritten(1000);
        }, [&](API::Client& client) { client.StopTests(&rpcOK); }, peer);
        qInfo() << "split_frame elapsed_ms=" << elapsed << "request_bytes=" << peer.requestBytes;
        return elapsed >= 0 && elapsed < 500 && rpcOK;
    }

    bool testBadInbound(const char* name, quint8 status, quint32 dataLength) {
        PeerResult peer;
        bool firstOK = false;
        bool secondOK = false;
        const auto elapsed = runExchange([=](QLocalSocket& socket, const QByteArray& request, PeerResult& result) {
            QByteArray requests = request;
            while (requests.size() < 36 && socket.waitForReadyRead(500)) requests += socket.readAll();
            result.requestBytes = requests.size();
            socket.write(responseHeader(requestId(requests), status, dataLength));
            socket.waitForBytesWritten(1000);
            if (socket.state() != QLocalSocket::UnconnectedState) {
                result.disconnected = socket.waitForDisconnected(1500);
            } else {
                result.disconnected = true;
            }
        }, [&](API::Client& client) {
            auto* first = QThread::create([&] { client.StopTests(&firstOK); });
            auto* second = QThread::create([&] { client.StopTests(&secondOK); });
            first->start();
            second->start();
            first->wait();
            second->wait();
            delete first;
            delete second;
        }, peer);
        qInfo() << name << "elapsed_ms=" << elapsed << "request_bytes=" << peer.requestBytes
                << "peer_disconnected=" << peer.disconnected;
        return elapsed >= 0 && elapsed < 500 && peer.requestBytes >= 36
            && !firstOK && !secondOK && peer.disconnected;
    }

    bool testDisconnectWakesCall() {
        PeerResult peer;
        bool rpcOK = false;
        const auto elapsed = runExchange([](QLocalSocket& socket, const QByteArray&, PeerResult&) {
            socket.abort();
        }, [&](API::Client& client) { client.StopTests(&rpcOK); }, peer);
        qInfo() << "disconnect elapsed_ms=" << elapsed;
        return elapsed >= 0 && elapsed < 500 && !rpcOK;
    }

    bool testOversizedOutbound() {
        PeerResult peer;
        bool rpcOK = false;
        const QString oversized(kMaxPayloadSize, QLatin1Char('x'));
        const auto elapsed = runExchange([](QLocalSocket&, const QByteArray&, PeerResult&) {},
            [&](API::Client& client) { (void) client.CheckConfig(&rpcOK, oversized); }, peer, 500);
        qInfo() << "outbound_oversize elapsed_ms=" << elapsed << "socket_bytes=" << peer.requestBytes;
        return elapsed >= 0 && elapsed < 500 && !rpcOK && peer.requestBytes == 0;
    }

    bool testQueryState() {
        PeerResult peer;
        bool rpcOK = false;
        libcore::CoreStateResponse state;
        const auto elapsed = runExchange([](QLocalSocket& socket, const QByteArray& request, PeerResult&) {
            const QByteArray payload = QByteArray::fromHex("0801102a");
            socket.write(responseHeader(requestId(request), 0, payload.size()) + payload);
            socket.waitForBytesWritten(1000);
        }, [&](API::Client& client) { state = client.QueryState(&rpcOK); }, peer);
        return elapsed >= 0 && elapsed < 500 && rpcOK
            && state.running.value_or(false) && state.profile_id.value_or(-1) == 42;
    }

    bool testImmediateCallAfterReconnect() {
        const QString serverName = "trpc-" + QUuid::createUuid().toString(QUuid::Id128).first(8);
        std::promise<bool> listeningPromise;
        auto listening = listeningPromise.get_future();

        auto* peer = QThread::create([&] {
            QLocalServer server;
            QLocalServer::removeServer(serverName);
            const bool started = server.listen(serverName);
            listeningPromise.set_value(started);
            if (!started) return;

            for (int profileId : {41, 42}) {
                if (!server.waitForNewConnection(2000)) return;
                std::unique_ptr<QLocalSocket> socket(server.nextPendingConnection());
                QByteArray request;
                if (!waitForRequest(*socket, request, 2000)) return;
                const QByteArray payload = QByteArray::fromHex(
                    profileId == 41 ? "08011029" : "0801102a");
                socket->write(responseHeader(requestId(request), 0, payload.size()) + payload);
                socket->waitForBytesWritten(1000);
            }
            QLocalServer::removeServer(serverName);
        });
        peer->start();
        if (!listening.get()) {
            peer->wait();
            delete peer;
            return false;
        }

        API::Client client;
        bool passed = true;
        for (int profileId : {41, 42}) {
            auto* socket = new QLocalSocket;
            socket->connectToServer(serverName);
            if (!socket->waitForConnected(2000)) {
                delete socket;
                passed = false;
                break;
            }
            client.Reconnect(socket);
            bool rpcOK = false;
            const auto state = client.QueryState(&rpcOK);
            passed &= rpcOK && state.profile_id.value_or(-1) == profileId;
        }

        peer->wait();
        delete peer;
        return passed;
    }
}

int main(int argc, char** argv) {
    QCoreApplication app(argc, argv);
    MW_show_log = [](const QString&) {};
    using OptionalString = std::optional<std::string>;
    if (!expectFailure(7, false, {}, {}, {}, "IPC call failed (code 7).")) return EXIT_FAILURE;
    if (!expectFailure(0, false, {}, {}, {}, "IPC response could not be decoded.")) return EXIT_FAILURE;
    if (!expectFailure(0, true, "private", "public", "domain error", "domain error")) return EXIT_FAILURE;
    if (!expectFailure(0, true, {}, "public", {}, "Core returned an empty key pair.")) return EXIT_FAILURE;
    if (!expectFailure(0, true, "private", OptionalString{std::string{}}, {}, "Core returned an empty key pair.")) return EXIT_FAILURE;

    const auto result = API::detail::NormalizeWgKeyPairResult(
        0, true, "private", "public", OptionalString{std::string{}});
    if (!result.ok() || result.privateKey != "private" || result.publicKey != "public" || !result.error.isEmpty()) {
        qCritical() << "Valid key pair was not normalized";
        return EXIT_FAILURE;
    }

    bool passed = testSplitFrame();
    passed &= testBadInbound("malformed_header", 2, 0);
    passed &= testBadInbound("inbound_oversize", 0, kMaxPayloadSize + 1);
    passed &= testDisconnectWakesCall();
    passed &= testOversizedOutbound();
    passed &= testQueryState();
    passed &= testImmediateCallAfterReconnect();
    return passed ? EXIT_SUCCESS : EXIT_FAILURE;
}
