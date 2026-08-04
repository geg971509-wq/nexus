#pragma once

#include <functional>
#include <mutex>
#include <stop_token>
#include <thread>
#include <utility>

namespace Throne {

// Owns a long-lived background task and makes its stop/join boundary explicit.
// Callers provide a wake callback for tasks blocked on a condition variable.
class StoppableWorker {
public:
    using Task = std::function<void(std::stop_token)>;

    StoppableWorker() = default;
    StoppableWorker(const StoppableWorker&) = delete;
    StoppableWorker& operator=(const StoppableWorker&) = delete;

    ~StoppableWorker() { Stop(); }

    void Start(Task task) {
        std::lock_guard lock(mutex_);
        if (worker_.joinable()) return;
        worker_ = std::jthread(std::move(task));
    }

    void Stop(const std::function<void()>& wake = {}) {
        std::lock_guard lock(mutex_);
        if (!worker_.joinable()) return;
        worker_.request_stop();
        if (wake) wake();
        worker_.join();
    }

    [[nodiscard]] bool joinable() const {
        std::lock_guard lock(mutex_);
        return worker_.joinable();
    }

private:
    mutable std::mutex mutex_;
    std::jthread worker_;
};

} // namespace Throne
