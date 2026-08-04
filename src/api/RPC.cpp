#include "include/api/RPC.h"
#include <utility>

#include "include/global/Configs.hpp"

#include <QLocalSocket>
#include <QDataStream>
#include <QAtomicInt>
#include <QMap>
#include <QMutex>
#include <QObject>
#include <QThread>

#include <mutex>
#include <condition_variable>
#include <chrono>
#include <atomic>
#include <exception>
#include <functional>
#include <limits>
#include <memory>

namespace API {

    // -----------------------------------------------------------------------
    // LocalSocketChannel — multiplexed framing over QLocalSocket
    //
    // Request wire format (little-endian):
    //   [uint32: request ID]
    //   [uint16: method name length][method name bytes]
    //   [uint32: payload length][payload bytes]
    //
    // Response wire format (little-endian):
    //   [uint32: request ID]
    //   [uint8:  status (0=OK, 1=error)]
    //   [uint32: data length][data bytes]
    //
    // The channel is created once and lives for the whole app. On core
    // restart the *connection* is replaced via Reconnect(): the old socket
    // is torn down and the new one adopted, all on the io_thread, while the
    // channel object (and therefore `defaultClient`) stays stable so that
    // worker threads calling into it never touch freed memory.
    //
    // `sock` and `read_buf` are io_thread-only state. All socket operations
    // are dispatched as queued lambdas to `io_anchor`, a stable QObject that
    // lives on io_thread for the channel's entire lifetime — callers never
    // dereference `sock` directly, so swapping it cannot race with a write.
    //
    // PendingCall is owned through a shared_ptr held jointly by Call() and
    // the `pending` map, so a timed-out / spuriously-woken Call() can never
    // destroy it while the io_thread is still writing to it or notifying.
    // -----------------------------------------------------------------------
    class Client::LocalSocketChannel {

        struct PendingCall {
            std::mutex          mu;
            std::condition_variable cv;
            bool                done   = false;
            quint8              status = 1;     // default: error
            QByteArray          data;
        };

        QThread      *io_thread;
        QObject      *io_anchor;          // stable dispatch target on io_thread
        QLocalSocket *sock = nullptr;     // io_thread only
        QByteArray    read_buf;           // io_thread only

        QAtomicInt    next_id{1};         // monotonic across reconnects
        std::mutex    pending_mu;
        QMap<quint32, std::shared_ptr<PendingCall>> pending;
        quint64 connection_generation = 0;

        std::atomic<bool> connected_{false};

        static constexpr int kIOTimeoutMs = 30000;
        static constexpr quint64 kMaxPayloadSize = 16ULL * 1024ULL * 1024ULL;
        static constexpr qint64 kResponseHeaderSize = 9;

        // Called on io_thread via readyRead
        void onReadyRead() {
            if (sock == nullptr) return;
            read_buf += sock->readAll();
            processBuffer();
        }

        void wakeAllWithError() {
            connected_.store(false, std::memory_order_release);
            std::lock_guard<std::mutex> lock(pending_mu);
            ++connection_generation;
            for (auto &call : pending) {
                std::lock_guard<std::mutex> cg(call->mu);
                call->done = true;
                call->cv.notify_one();
            }
            pending.clear();
        }

        void failConnection() {
            read_buf.clear();
            if (sock) sock->abort();
            wakeAllWithError();
        }

        // io_thread only
        void processBuffer() {
            // Parse as many complete response frames as possible.
            // Response header: 4 (reqId) + 1 (status) + 4 (dataLen) = 9 bytes
            while (read_buf.size() >= kResponseHeaderSize) {
                quint32 reqId, dataLen;
                quint8  status;
                QDataStream::Status streamStatus;
                {
                    QDataStream ds(read_buf);
                    ds.setByteOrder(QDataStream::LittleEndian);
                    ds >> reqId >> status >> dataLen;
                    streamStatus = ds.status();
                }

                if (streamStatus != QDataStream::Ok || status > 1 || dataLen > kMaxPayloadSize) {
                    failConnection();
                    return;
                }

                const qint64 totalSize = kResponseHeaderSize + dataLen;
                if (read_buf.size() < totalSize) break;  // frame not yet complete

                QByteArray data = read_buf.mid(kResponseHeaderSize, static_cast<int>(dataLen));
                read_buf.remove(0, totalSize);

                std::shared_ptr<PendingCall> call;
                {
                    std::lock_guard<std::mutex> lock(pending_mu);
                    call = pending.value(reqId, nullptr);
                    if (call) pending.remove(reqId);
                }
                if (call) {
                    // Hold call->mu across the notify so the cv/mutex cannot
                    // be destroyed by a waking Call() mid-notification.
                    std::lock_guard<std::mutex> cg(call->mu);
                    call->status = status;
                    call->data   = std::move(data);
                    call->done   = true;
                    call->cv.notify_one();
                }
            }
        }

    public:
        LocalSocketChannel() {
            io_thread = new QThread;
            io_anchor = new QObject;
            io_anchor->moveToThread(io_thread);
            io_thread->start();
        }

        ~LocalSocketChannel() {
            wakeAllWithError();

            // Close socket and stop event loop on io_thread
            QMetaObject::invokeMethod(io_anchor, [this]() {
                if (sock) {
                    sock->close();
                    delete sock;
                    sock = nullptr;
                }
                io_thread->quit();
            }, Qt::QueuedConnection);

            io_thread->wait();
            delete io_anchor;   // safe: io_thread has finished
            delete io_thread;
        }

        // Replace the underlying connection. Must be called on the thread that
        // currently owns `newSock` (the UI thread, same as before).
        void Reconnect(QLocalSocket *newSock) {
            // Fail every in-flight call so blocked Call()s return an error.
            wakeAllWithError();

            // Hand the socket over to the io_thread.
            newSock->setParent(nullptr);
            newSock->moveToThread(io_thread);

            // Complete the swap before callers can observe the new connection.
            QMetaObject::invokeMethod(io_anchor, [this, newSock]() {
                if (sock) {
                    sock->disconnect(io_anchor);   // drop old readyRead/disconnected
                    sock->close();
                    sock->deleteLater();
                }
                sock = newSock;
                sock->setReadBufferSize(kResponseHeaderSize + kMaxPayloadSize);
                read_buf.clear();
                QObject::connect(sock, &QLocalSocket::readyRead, io_anchor,
                    [this]() { onReadyRead(); });
                QObject::connect(sock, &QLocalSocket::disconnected, io_anchor,
                    [this]() { wakeAllWithError(); });
                {
                    std::lock_guard<std::mutex> lock(pending_mu);
                    connected_.store(true, std::memory_order_release);
                }
                if (sock->bytesAvailable() > 0) onReadyRead();
            }, Qt::BlockingQueuedConnection);
        }

        // Returns 0 on success, non-zero on failure. -2 = canceled.
        int Call(const QString &methodName, const std::string &req,
                 std::vector<uint8_t> &rsp, int timeout_ms = 0,
                 const std::function<bool()> &canceled = {}) {
            if (!connected_.load(std::memory_order_acquire)) return -1919;

            const int ms = (timeout_ms > 0) ? timeout_ms : kIOTimeoutMs;

            const auto methodBytes = methodName.toUtf8();
            const auto methodSize = static_cast<quint64>(methodBytes.size());
            const auto requestSize = static_cast<quint64>(req.size());
            if (methodSize > std::numeric_limits<quint16>::max()
                || requestSize > kMaxPayloadSize
                || methodSize > kMaxPayloadSize - requestSize) {
                return 1;
            }

            const quint32 reqId = static_cast<quint32>(next_id.fetchAndAddOrdered(1));
            const auto reqBytes = QByteArray::fromStdString(req);

            QByteArray frame;
            {
                QDataStream ds(&frame, QIODevice::WriteOnly);
                ds.setByteOrder(QDataStream::LittleEndian);
                ds << reqId;
                ds << static_cast<quint16>(methodBytes.size());
                ds.writeRawData(methodBytes.constData(), methodBytes.size());
                ds << static_cast<quint32>(reqBytes.size());
                ds.writeRawData(reqBytes.constData(), reqBytes.size());
            }

            // Register before sending (never miss the response).
            auto call = std::make_shared<PendingCall>();
            quint64 generation;
            {
                std::lock_guard<std::mutex> lock(pending_mu);
                if (!connected_.load(std::memory_order_acquire)) return -1919;
                generation = connection_generation;
                pending[reqId] = call;
            }

            // Dispatch write through the stable io anchor (FIFO, never
            // touches `sock` on this thread).
            QMetaObject::invokeMethod(io_anchor, [this, frame, reqId, generation]() {
                std::shared_ptr<PendingCall> staleCall;
                bool currentConnection;
                {
                    std::lock_guard<std::mutex> lock(pending_mu);
                    currentConnection = generation == connection_generation;
                    if (!currentConnection) staleCall = pending.take(reqId);
                }
                if (staleCall) {
                    std::lock_guard<std::mutex> lock(staleCall->mu);
                    staleCall->done = true;
                    staleCall->cv.notify_one();
                } else if (currentConnection && sock) {
                    sock->write(frame);
                }
            }, Qt::QueuedConnection);

            // Wait for response
            std::unique_lock<std::mutex> lock(call->mu);
            bool ok = call->cv.wait_for(lock,
                std::chrono::milliseconds(ms),
                [&call, &canceled] { return call->done || (canceled && canceled()); });
            const bool wasDone = call->done;
            lock.unlock();   // never hold call->mu while taking pending_mu

            if (ok && !wasDone) {
                // Woke because canceled, not because a response arrived.
                std::lock_guard<std::mutex> plock(pending_mu);
                pending.remove(reqId);
                return -2;
            }

            if (!ok) {
                // Timed out — reclaim our slot unless processBuffer took it.
                bool claimedByReader = false;
                {
                    std::lock_guard<std::mutex> plock(pending_mu);
                    if (pending.remove(reqId) == 0) claimedByReader = true;
                }
                if (claimedByReader) {
                    // A response is inbound; give it a brief bounded chance.
                    lock.lock();
                    ok = call->cv.wait_for(lock,
                        std::chrono::milliseconds(ms),
                        [&call] { return call->done; });
                    lock.unlock();
                }
            }

            std::lock_guard<std::mutex> g(call->mu);
            if (!ok || call->status != 0) {
                if (ok && call->status != 0) {
                    MW_show_log("[Core error] " + QString::fromUtf8(call->data));
                    // Surface the core's error payload to the caller too, so it can be
                    // inspected instead of just logged (e.g. detecting missing Xray geo
                    // assets when a Test/IPTest/SpeedTest RPC fails). Callers only read
                    // `rsp` when the call succeeds, so this is inert for the rest.
                    rsp.assign(call->data.begin(), call->data.end());
                }
                return 1;
            }
            rsp.assign(call->data.begin(), call->data.end());
            return 0;
        }
    };

    // -----------------------------------------------------------------------
    // Client
    // -----------------------------------------------------------------------

    namespace {
        // spb throws std::runtime_error on any malformed/torn input. These
        // calls run on worker QThreads, so an uncaught throw would terminate
        // the whole process. Turn a bad frame into a failed RPC instead.
        template <typename T>
        bool tryDeserialize(const std::vector<uint8_t> &resp, T &out) {
            try {
                out = spb::pb::deserialize<T>(resp);
                return true;
            } catch (const std::exception &e) {
                MW_show_log(QString("[RPC] dropped malformed response: ") + e.what());
                return false;
            } catch (...) {
                MW_show_log("[RPC] dropped malformed response");
                return false;
            }
        }

        template <typename Request, typename Reply, typename Channel>
        bool callTyped(Channel& channel, bool* rpcOK, const QString& method,
                       const Request& request, Reply& reply) {
            std::vector<uint8_t> response;
            const auto status = channel.Call(
                method, spb::pb::serialize<std::string>(request), response);
            if (status == 0 && tryDeserialize(response, reply)) {
                *rpcOK = true;
                return true;
            }
            *rpcOK = false;
            MW_show_log(QString("IPC call failed (code %1)\n").arg(status));
            return false;
        }
    }

    Client::~Client() = default;

    Client::Client() {
        this->channel = std::make_unique<LocalSocketChannel>();
    }

    void Client::Reconnect(QLocalSocket *socket) {
        channel->Reconnect(socket);
    }

    WgKeyPairResult detail::NormalizeWgKeyPairResult(
        int status,
        bool decoded,
        const std::optional<std::string>& privateKey,
        const std::optional<std::string>& publicKey,
        const std::optional<std::string>& error) {
        if (status != 0) return {{}, {}, QString("IPC call failed (code %1).").arg(status)};
        if (!decoded) return {{}, {}, "IPC response could not be decoded."};
        if (error && !error->empty()) return {{}, {}, QString::fromStdString(*error)};
        if (!privateKey || privateKey->empty() || !publicKey || publicKey->empty()) {
            return {{}, {}, "Core returned an empty key pair."};
        }
        return {QString::fromStdString(*privateKey), QString::fromStdString(*publicKey), {}};
    }

#define CALL_OK 0

#define NOT_OK      \
    *rpcOK = false; \
    MW_show_log(QString("IPC call failed (code %1)\n").arg(status));

    QString Client::Start(bool *rpcOK, const libcore::LoadConfigReq &request,
                          const std::function<bool()> &canceled) {
        libcore::ErrorResp reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("Start", spb::pb::serialize<std::string>(request), resp, 0, canceled);

        if (status == -2) { *rpcOK = false; return {}; } // canceled
        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            *rpcOK = true;
            return QString::fromStdString(reply.error.value());
        } else {
            NOT_OK
            return "";
        }
    }

    QString Client::Stop(bool *rpcOK) {
        libcore::EmptyReq request;
        libcore::ErrorResp reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("Stop", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            *rpcOK = true;
            return QString::fromStdString(reply.error.value());
        } else {
            NOT_OK
            return "";
        }
    }

    libcore::QueryStatsResp Client::QueryStats() {
        libcore::EmptyReq request;
        libcore::QueryStatsResp reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("QueryStats", spb::pb::serialize<std::string>(request), resp, 500);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            return reply;
        }
        return {};
    }

    libcore::TestResp Client::Test(bool *rpcOK, const libcore::TestReq &request, QString *coreError) {
        libcore::TestResp reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("Test", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            *rpcOK = true;
            return reply;
        } else {
            if (coreError && !resp.empty())
                *coreError = QString::fromUtf8(reinterpret_cast<const char *>(resp.data()), static_cast<int>(resp.size()));
            NOT_OK
            return {};
        }
    }

    void Client::StopTests(bool *rpcOK) {
        const libcore::EmptyReq request;
        std::vector<uint8_t> resp;
        auto status = channel->Call("StopTest", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK) {
            *rpcOK = true;
        } else {
            NOT_OK
        }
    }

    libcore::QueryURLTestResponse Client::QueryURLTest(bool *rpcOK)
    {
        const libcore::EmptyReq request;
        libcore::QueryURLTestResponse reply;
        if (callTyped<libcore::EmptyReq, libcore::QueryURLTestResponse>(
                *channel, rpcOK, "QueryURLTest", request, reply)) return reply;
        return {};
    }

    libcore::IPTestResp Client::IPTest(bool *rpcOK, const libcore::IPTestRequest &request, QString *coreError) {
        libcore::IPTestResp reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("IPTest", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            *rpcOK = true;
            return reply;
        } else {
            if (coreError && !resp.empty())
                *coreError = QString::fromUtf8(reinterpret_cast<const char *>(resp.data()), static_cast<int>(resp.size()));
            NOT_OK
            return {};
        }
    }

    libcore::QueryIPTestResponse Client::QueryIPTest(bool *rpcOK) {
        const libcore::EmptyReq request;
        libcore::QueryIPTestResponse reply;
        if (callTyped<libcore::EmptyReq, libcore::QueryIPTestResponse>(
                *channel, rpcOK, "QueryIPTest", request, reply)) return reply;
        return {};
    }

    libcore::GetDefaultInterfaceResponse Client::GetDefaultInterface(bool *rpcOK) const {
        libcore::EmptyReq request;
        libcore::GetDefaultInterfaceResponse reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("GetDefaultInterface", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            *rpcOK = true;
            return reply;
        } else {
            NOT_OK
            return {};
        }
    }

    libcore::QueryAutoSelectorsResponse Client::QueryAutoSelectors(bool *rpcOK) const {
        libcore::EmptyReq request;
        libcore::QueryAutoSelectorsResponse reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("QueryAutoSelectors", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            *rpcOK = true;
            return reply;
        } else {
            NOT_OK
            return {};
        }
    }

    QString Client::AutoSelectorAction(bool *rpcOK, const QString &tag, const QString &action,
                                       const QString &member) const {
        libcore::AutoSelectorActionRequest request;
        request.tag = tag.toStdString();
        request.action = action.toStdString();
        request.member = member.toStdString();
        libcore::ErrorResp reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("AutoSelectorAction", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            *rpcOK = true;
            return QString::fromStdString(reply.error.value());
        } else {
            NOT_OK
            return "IPC error";
        }
    }

    QString Client::SetSystemDNS(bool *rpcOK, const bool clear) const {
        libcore::SetSystemDNSRequest request{clear};
        std::vector<uint8_t> resp;
        auto status = channel->Call("SetSystemDNS", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK) {
            *rpcOK = true;
            return "";
        } else {
            NOT_OK
            return "IPC error";
        }
    }

    libcore::QueryConnectionsResp Client::QueryConnections() const
    {
        libcore::EmptyReq request;
        libcore::QueryConnectionsResp reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("QueryConnections", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            return reply;
        }
        if (status != CALL_OK) MW_show_log("Failed to query connections: IPC error");
        return {};
    }

    libcore::CoreStateResponse Client::QueryState(bool *rpcOK) const {
        const libcore::EmptyReq request;
        libcore::CoreStateResponse reply;
        if (callTyped<libcore::EmptyReq, libcore::CoreStateResponse>(
                *channel, rpcOK, "QueryState", request, reply)) return reply;
        return {};
    }

    QString Client::CheckConfig(bool* rpcOK, const QString& config, bool isXray) const
    {
        libcore::LoadConfigReq request;
        if (isXray)
        {
            request.need_xray = true;
            request.xray_config = config.toStdString();
        } else
        {
            request.core_config = config.toStdString();
        }
        libcore::ErrorResp reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("CheckConfig", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply))
        {
            *rpcOK = true;
            return QString::fromStdString(reply.error.value());
        } else
        {
            NOT_OK
            return "IPC error";
        }
    }

    bool Client::IsPrivileged(bool* rpcOK) const
    {
        libcore::EmptyReq request;
        libcore::IsPrivilegedResponse reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("IsPrivileged", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply))
        {
            *rpcOK = true;
            return reply.has_privilege.value();
        } else
        {
            NOT_OK
            return false;
        }
    }

    libcore::SpeedTestResponse Client::SpeedTest(bool *rpcOK, const libcore::SpeedTestRequest &request, QString *coreError)
    {
        libcore::SpeedTestResponse reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("SpeedTest", spb::pb::serialize<std::string>(request), resp);

        if (status == CALL_OK && tryDeserialize(resp, reply)) {
            *rpcOK = true;
            return reply;
        } else {
            if (coreError && !resp.empty())
                *coreError = QString::fromUtf8(reinterpret_cast<const char *>(resp.data()), static_cast<int>(resp.size()));
            NOT_OK
            return {};
        }
    }

    libcore::QuerySpeedTestResponse Client::QueryCurrentSpeedTests(bool *rpcOK)
    {
        const libcore::EmptyReq request;
        libcore::QuerySpeedTestResponse reply;
        if (callTyped<libcore::EmptyReq, libcore::QuerySpeedTestResponse>(
                *channel, rpcOK, "QuerySpeedTest", request, reply)) return reply;
        return {};
    }

    libcore::QueryCountryTestResponse Client::QueryCountryTestResults(bool* rpcOK)
    {
        const libcore::EmptyReq request;
        libcore::QueryCountryTestResponse reply;
        if (callTyped<libcore::EmptyReq, libcore::QueryCountryTestResponse>(
                *channel, rpcOK, "QueryCountryTest", request, reply)) return reply;
        return {};
    }

    WgKeyPairResult Client::GenWgKeyPair()
    {
        const libcore::EmptyReq request;
        libcore::GenWgKeyPairResponse reply;
        std::vector<uint8_t> resp;
        auto status = channel->Call("GenWgKeyPair", spb::pb::serialize<std::string>(request), resp);
        const bool decoded = status == CALL_OK && tryDeserialize(resp, reply);
        return detail::NormalizeWgKeyPairResult(
            status, decoded, reply.private_key, reply.public_key, reply.error);
    }

} // namespace API
