#pragma once

#include <QMutex>
#include <QMutexLocker>
#include <QWaitCondition>

#include <stop_token>

namespace Throne {

inline bool waitForStopOrTimeout(QWaitCondition &condition, QMutex &mutex,
                                 std::stop_token stopToken, unsigned long timeoutMs) {
    QMutexLocker lock(&mutex);
    if (stopToken.stop_requested()) return false;
    condition.wait(&mutex, timeoutMs);
    return !stopToken.stop_requested();
}

} // namespace Throne
