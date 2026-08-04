#pragma once

#include <atomic>
#include <cstdint>
#include <exception>

namespace Throne {

enum class OperationState : std::uint8_t {
    Idle,
    Running,
    Starting,
    Stopping,
};

class OperationGate final {
public:
    bool tryBegin(OperationState operation) noexcept
    {
        auto expected = OperationState::Idle;
        return state_.compare_exchange_strong(
            expected, operation, std::memory_order_acq_rel, std::memory_order_acquire);
    }

    void finish(OperationState operation) noexcept
    {
        auto expected = operation;
        if (!state_.compare_exchange_strong(
                expected, OperationState::Idle, std::memory_order_acq_rel, std::memory_order_acquire)) {
            std::terminate();
        }
    }

    [[nodiscard]] OperationState state() const noexcept
    {
        return state_.load(std::memory_order_acquire);
    }

private:
    std::atomic<OperationState> state_{OperationState::Idle};
};

}
