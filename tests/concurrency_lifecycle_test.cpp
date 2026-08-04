#include "include/ui/utils/OperationGate.h"
#include "include/global/InterruptibleWait.hpp"
#include "include/global/StoppableWorker.hpp"

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <latch>
#include <mutex>
#include <thread>
#include <vector>

namespace {

bool stoppableWorkerJoinsAndRestarts()
{
    Throne::StoppableWorker worker;
    std::mutex mutex;
    std::condition_variable condition;
    std::atomic<int> starts{0};
    std::atomic<int> stops{0};

    auto run = [&](std::stop_token stopToken) {
        starts.fetch_add(1, std::memory_order_release);
        condition.notify_all();
        std::unique_lock lock(mutex);
        condition.wait(lock, [&] { return stopToken.stop_requested(); });
        stops.fetch_add(1, std::memory_order_release);
    };

    worker.Start(run);
    worker.Start(run);
    {
        std::unique_lock lock(mutex);
        if (!condition.wait_for(lock, std::chrono::seconds(1), [&] {
                return starts.load(std::memory_order_acquire) == 1;
            })) return false;
    }
    worker.Stop([&] { condition.notify_all(); });
    if (worker.joinable() || stops.load(std::memory_order_acquire) != 1) return false;

    worker.Start(run);
    {
        std::unique_lock lock(mutex);
        if (!condition.wait_for(lock, std::chrono::seconds(1), [&] {
                return starts.load(std::memory_order_acquire) == 2;
            })) return false;
    }
    worker.Stop([&] { condition.notify_all(); });
    return !worker.joinable() && stops.load(std::memory_order_acquire) == 2;
}

bool immediateStopCannotMissWake()
{
    using namespace std::chrono;
    const auto startedAt = steady_clock::now();
    for (int attempt = 0; attempt < 100; ++attempt) {
        Throne::StoppableWorker worker;
        QMutex mutex;
        QWaitCondition condition;
        worker.Start([&](std::stop_token stopToken) {
            Throne::waitForStopOrTimeout(condition, mutex, stopToken, 60'000);
        });
        worker.Stop([&] {
            QMutexLocker lock(&mutex);
            condition.wakeAll();
        });
        if (worker.joinable()) return false;
    }
    return steady_clock::now() - startedAt < seconds(2);
}

} // namespace

int main()
{
    if (!stoppableWorkerJoinsAndRestarts()) return EXIT_FAILURE;
    if (!immediateStopCannotMissWake()) return EXIT_FAILURE;

    Throne::OperationGate profile;
    if (!profile.tryBegin(Throne::OperationState::Starting)) return EXIT_FAILURE;
    if (profile.tryBegin(Throne::OperationState::Stopping)) return EXIT_FAILURE;
    if (profile.state() != Throne::OperationState::Starting) return EXIT_FAILURE;
    profile.finish(Throne::OperationState::Starting);
    if (profile.state() != Throne::OperationState::Idle) return EXIT_FAILURE;

    if (!profile.tryBegin(Throne::OperationState::Stopping)) return EXIT_FAILURE;
    if (profile.tryBegin(Throne::OperationState::Starting)) return EXIT_FAILURE;
    profile.finish(Throne::OperationState::Stopping);
    if (profile.state() != Throne::OperationState::Idle) return EXIT_FAILURE;

    Throne::OperationGate test;
    if (!test.tryBegin(Throne::OperationState::Running)) return EXIT_FAILURE;
    if (test.tryBegin(Throne::OperationState::Running)) return EXIT_FAILURE;

    constexpr int taskCount = 4;
    std::latch completed(taskCount);
    std::atomic<int> ran = 0;
    std::vector<std::thread> workers;
    for (int i = 0; i < taskCount; ++i) {
        workers.emplace_back([&] {
            ++ran;
            completed.count_down();
        });
    }

    if (test.state() != Throne::OperationState::Running) return EXIT_FAILURE;
    completed.wait();
    for (auto &worker : workers) worker.join();
    test.finish(Throne::OperationState::Running);
    if (ran.load() != taskCount) return EXIT_FAILURE;
    if (test.state() != Throne::OperationState::Idle) return EXIT_FAILURE;
    return EXIT_SUCCESS;
}
