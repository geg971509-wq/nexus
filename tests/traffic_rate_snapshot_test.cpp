#include <QMutex>
#include <QMutexLocker>
#include "include/stats/traffic/TrafficRateSnapshot.hpp"

#include <atomic>
#include <chrono>
#include <cstdlib>
#include <thread>
#include <vector>

namespace {

class RatePublisher {
public:
    void publish(const Stats::TrafficRateSnapshot& snap) {
        QMutexLocker lock(&mutex_);
        published_ = snap;
    }

    Stats::TrafficRateSnapshot get() const {
        QMutexLocker lock(&mutex_);
        return published_;
    }

private:
    mutable QMutex mutex_;
    Stats::TrafficRateSnapshot published_;
};

bool concurrentPublishGetIsStable() {
    RatePublisher pub;
    std::atomic<bool> stop{false};
    std::atomic<bool> bad{false};

    std::thread writer([&] {
        double v = 0;
        while (!stop.load(std::memory_order_acquire)) {
            pub.publish({v, v + 1, v + 2, v + 3});
            v += 1.0;
        }
    });

    std::vector<std::thread> readers;
    readers.reserve(4);
    for (int i = 0; i < 4; ++i) {
        readers.emplace_back([&] {
            while (!stop.load(std::memory_order_acquire)) {
                const auto s = pub.get();
                if (!(s.proxy_uplink == s.proxy_downlink + 1
                      && s.direct_downlink == s.proxy_downlink + 2
                      && s.direct_uplink == s.proxy_downlink + 3)) {
                    bad.store(true, std::memory_order_release);
                    return;
                }
            }
        });
    }

    std::this_thread::sleep_for(std::chrono::milliseconds(150));
    stop.store(true, std::memory_order_release);
    writer.join();
    for (auto& t : readers) t.join();
    return !bad.load(std::memory_order_acquire);
}

} // namespace

int main() {
    if (!concurrentPublishGetIsStable()) return EXIT_FAILURE;
    return EXIT_SUCCESS;
}
