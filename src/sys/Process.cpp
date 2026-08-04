#include "include/sys/Process.hpp"
#include "include/global/Configs.hpp"
#include "include/global/Utils.hpp"
#include "include/sys/macos/MacOS.h"

#include <QApplication>
#include <QDir>
#include <QEventLoop>
#include <QFile>
#include <QFileInfo>
#include <QFileSystemWatcher>
#include <QMetaObject>
#include <QMutex>
#include <QProcess>
#include <QStandardPaths>
#include <QThread>
#include <QWaitCondition>

#include <atomic>
#include <memory>

#ifdef Q_OS_WIN
#include <windows.h>
#else
#include <cerrno>
#include <csignal>
#include <cstring>
#include <fcntl.h>
#include <sys/wait.h>
#include <unistd.h>
#include <vector>
// POSIX does not put environ in a header; libc provides it.
extern char **environ;
#endif

namespace Configs_sys {
    namespace {
        // Elevated osascript QProcess is UI-thread only. Workers only track its pid.
        QProcess *elevatedUpProcess = nullptr;
        // pending + offset are touched from the UI watcher and worker threads
        // (Down/Status). One mutex serializes both so QString is not mutated
        // concurrently — same class of bug as the old Profile-field race.
        QMutex elevatedUpLogMutex;
        QString elevatedUpLogPending;
        qint64 elevatedUpLogOffset = 0;
        QMutex elevatedUpResultMutex;
        QWaitCondition elevatedUpResultReady;
        QString elevatedUpExitError;
        std::atomic<qint64> elevatedUpPid{0};
        std::atomic_bool elevatedUpExternal{false};
        std::atomic_bool elevatedUpExited{false};

        QString warpPath() { return QApplication::applicationDirPath() + "/warp-client"; }

        // Non-package builds leave GetBasePath() at the app dir, where the data
        // dir name lands on the warp-client binary itself. Fall back to the
        // per-user config dir in that case; packaged builds already resolve there.
        QString warpBase() {
            const auto base = Configs::GetBasePath();
            if (!WarpDataDirCollides(WarpDataDirIn(base), warpPath())) return base;
            return QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
        }

        QString warpDataDir() { return WarpDataDirIn(warpBase()); }

        QString warpLogPath() { return WarpLogPathIn(warpBase()); }

        void forwardWarpText(const QString &text, bool flush) {
            if (text.isEmpty() && !flush) return;
            // Drain under the lock, then emit outside it so MW_show_log cannot
            // re-enter this path while we still hold elevatedUpLogMutex.
            QStringList lines;
            {
                QMutexLocker lock(&elevatedUpLogMutex);
                elevatedUpLogPending += text;
                int newline = -1;
                while ((newline = elevatedUpLogPending.indexOf('\n')) >= 0) {
                    auto line = elevatedUpLogPending.left(newline).trimmed();
                    elevatedUpLogPending.remove(0, newline + 1);
                    if (!line.isEmpty()) lines.append(line);
                }
                if (flush && !elevatedUpLogPending.trimmed().isEmpty()) {
                    lines.append(elevatedUpLogPending.trimmed());
                    elevatedUpLogPending.clear();
                }
            }
            if (!MW_show_log) return;
            for (const auto &line : lines) MW_show_log("[WARP] " + line);
        }

        bool prepareWarpLog(QString *error) {
            const auto path = warpLogPath();
            const auto dirPath = QFileInfo(path).absolutePath();
            if (!QDir().mkpath(dirPath)) {
                if (error) *error = "failed to create WARP log directory";
                return false;
            }
            QFileInfo existing(path);
            if (existing.exists() && (existing.isSymLink() || !existing.isFile())) {
                if (error) *error = "WARP log path is not a regular file";
                return false;
            }
            QFile file(path);
            if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
                if (error) *error = file.errorString();
                return false;
            }
            if (!file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner)) {
                if (error) *error = "failed to set WARP log permissions";
                return false;
            }
            file.close();
            {
                QMutexLocker lock(&elevatedUpLogMutex);
                elevatedUpLogOffset = 0;
                elevatedUpLogPending.clear();
            }
            {
                QMutexLocker lock(&elevatedUpResultMutex);
                elevatedUpExitError.clear();
            }
            return true;
        }

        void readWarpLog(bool flush) {
            QFile file(warpLogPath());
            if (!file.open(QIODevice::ReadOnly)) {
                if (flush) forwardWarpText({}, true);
                return;
            }
            qint64 offset;
            {
                QMutexLocker lock(&elevatedUpLogMutex);
                offset = elevatedUpLogOffset;
            }
            const auto size = file.size();
            if (size < offset) offset = 0;
            if (!file.seek(offset)) return;
            while (!file.atEnd()) {
                const auto data = file.read(64 * 1024);
                if (data.isEmpty()) break;
                {
                    QMutexLocker lock(&elevatedUpLogMutex);
                    elevatedUpLogOffset = file.pos();
                }
                forwardWarpText(QString::fromLocal8Bit(data), false);
            }
            if (flush) forwardWarpText({}, true);
        }

        QProcessEnvironment warpEnvironment() {
            auto env = QProcessEnvironment::systemEnvironment();
            env.insert("THRONE_WARP_DATA_DIR", warpDataDir());
#ifdef Q_OS_MACOS
            env.insert("THRONE_WARP_OWNER_UID", QString::number(getuid()));
            env.insert("THRONE_WARP_OWNER_GID", QString::number(getgid()));
            env.insert("THRONE_WARP_LOG_PATH", warpLogPath());
#endif
            return env;
        }

        QString elevatedUpShellFragment() {
#ifdef Q_OS_MACOS
            // Run helper in foreground - it blocks until SIGTERM.
            // The osascript process stays alive tracking it.
            return BuildElevatedShell(warpPath(), "up", warpDataDir(), warpLogPath(),
                                      getuid(), getgid());
#else
            return {};
#endif
        }

        bool binaryTrusted(const QFileInfo &binary, QString *error) {
            if (!binary.exists() || !binary.isFile() || binary.isSymLink()
                || !(binary.permissions() & QFileDevice::ExeUser)) {
                if (error) *error = "warp-client is missing or not a regular executable";
                return false;
            }
            return true;
        }

#ifndef Q_OS_WIN
        bool pidAlive(qint64 pid) {
            if (pid <= 0) return false;
            return ::kill(static_cast<pid_t>(pid), 0) == 0 || errno != ESRCH;
        }

        void terminatePid(qint64 pid) {
            if (pid <= 0) return;
            ::kill(static_cast<pid_t>(pid), SIGTERM);
        }

        // Thread-safe short warp-client run: no QProcess (avoids CFSocket crashes off UI).
        bool runWarpPosix(const QString &command, QString *output, QString *error, int timeoutMs) {
            const auto program = warpPath();
            int pipefd[2];
            if (pipe(pipefd) != 0) {
                if (error) *error = QString::fromLocal8Bit(strerror(errno));
                return false;
            }

            const auto pathBytes = QFile::encodeName(program);
            const auto cmdBytes = command.toLocal8Bit();
            // Build envp before fork: setenv after fork is not async-signal-safe and
            // can deadlock if another thread holds libc env locks.
            std::vector<QByteArray> envStore;
            std::vector<char *> envp;
            for (char **e = environ; e && *e; ++e) {
                if (strncmp(*e, "THRONE_WARP_", 12) == 0) continue;
                envStore.emplace_back(*e);
            }
            envStore.push_back(QByteArray("THRONE_WARP_DATA_DIR=") + warpDataDir().toLocal8Bit());
#ifdef Q_OS_MACOS
            envStore.push_back(QByteArray("THRONE_WARP_OWNER_UID=") + QByteArray::number(static_cast<qint64>(getuid())));
            envStore.push_back(QByteArray("THRONE_WARP_OWNER_GID=") + QByteArray::number(static_cast<qint64>(getgid())));
#endif
            envp.reserve(envStore.size() + 1);
            for (auto &entry : envStore) envp.push_back(entry.data());
            envp.push_back(nullptr);

            const pid_t child = fork();
            if (child < 0) {
                close(pipefd[0]);
                close(pipefd[1]);
                if (error) *error = QString::fromLocal8Bit(strerror(errno));
                return false;
            }
            if (child == 0) {
                close(pipefd[0]);
                dup2(pipefd[1], STDOUT_FILENO);
                dup2(pipefd[1], STDERR_FILENO);
                if (pipefd[1] > STDERR_FILENO) close(pipefd[1]);
                char *const argv[] = {
                    const_cast<char *>(pathBytes.constData()),
                    const_cast<char *>(cmdBytes.constData()),
                    nullptr
                };
                execve(pathBytes.constData(), argv, envp.data());
                _exit(127);
            }

            close(pipefd[1]);
            fcntl(pipefd[0], F_SETFL, O_NONBLOCK);

            QByteArray collected;
            char buf[4096];
            int status = 0;
            bool timedOut = false;
            const int steps = qMax(1, timeoutMs / 50);
            for (int i = 0; i < steps; ++i) {
                for (;;) {
                    const ssize_t n = read(pipefd[0], buf, sizeof(buf));
                    if (n > 0) {
                        collected.append(buf, int(n));
                        continue;
                    }
                    break;
                }
                const pid_t waited = waitpid(child, &status, WNOHANG);
                if (waited == child) break;
                if (i + 1 == steps) {
                    timedOut = true;
                    ::kill(child, SIGKILL);
                    waitpid(child, &status, 0);
                    break;
                }
                QThread::msleep(50);
            }
            // Drain remaining output.
            for (;;) {
                const ssize_t n = read(pipefd[0], buf, sizeof(buf));
                if (n > 0) collected.append(buf, int(n));
                else break;
            }
            close(pipefd[0]);

            if (timedOut) {
                if (error) *error = QStringLiteral("warp-client timed out");
                return false;
            }
            const int code = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
            if (output) *output = QString::fromLocal8Bit(collected);
            if (code != 0) {
                if (error) {
                    auto err = QString::fromLocal8Bit(collected).trimmed();
                    if (err.isEmpty()) err = QStringLiteral("warp-client exited with code %1").arg(code);
                    *error = err;
                }
                return false;
            }
            return true;
        }
#else
        bool pidAlive(qint64) { return false; }
        void terminatePid(qint64) {}

        bool runWarpPosix(const QString &, QString *, QString *error, int) {
            if (error) *error = "warp-client control is not implemented on Windows";
            return false;
        }
#endif

        // UI-thread only: own the elevated QProcess and expose its pid to workers.
        void trackElevatedUpOnUi(QProcess *process) {
            if (process == nullptr) return;
            if (elevatedUpProcess == process) {
                elevatedUpPid = process->processId();
                return;
            }
            if (elevatedUpProcess != nullptr && elevatedUpProcess->state() == QProcess::NotRunning) {
                elevatedUpProcess->deleteLater();
            }
            elevatedUpProcess = process;
            elevatedUpPid = process->processId();
            elevatedUpExited = false;
            {
                QMutexLocker lock(&elevatedUpResultMutex);
                elevatedUpExitError.clear();
            }
            auto *logWatcher = new QFileSystemWatcher({warpLogPath()}, process);
            QObject::connect(logWatcher, &QFileSystemWatcher::fileChanged, qApp, [] {
                readWarpLog(false);
            });
            readWarpLog(false);
            QObject::connect(process, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished),
                             process, [process](int code, QProcess::ExitStatus status) {
                                 readWarpLog(true);
                                 const auto stdoutText = QString::fromLocal8Bit(process->readAllStandardOutput());
                                 const auto stderrText = QString::fromLocal8Bit(process->readAllStandardError());
                                 if (code != 0 || status != QProcess::NormalExit) {
                                     const auto detail = FormatWarpExitError(stderrText, stdoutText, code);
                                     {
                                         QMutexLocker lock(&elevatedUpResultMutex);
                                         elevatedUpExitError = detail;
                                         elevatedUpResultReady.wakeAll();
                                     }
                                     if (MW_show_log) MW_show_log("[WARP] " + detail);
                                 } else {
                                     // Log output even on successful exit to debug silent
                                     // failures. The helper redirects its own stdout/stderr
                                     // to warp.log, so the osascript pipes usually carry
                                     // nothing — skip whitespace-only noise lines.
                                     const auto outLine = stdoutText.trimmed();
                                     const auto errLine = stderrText.trimmed();
                                     if (MW_show_log) {
                                         if (!outLine.isEmpty()) MW_show_log("[WARP] stdout: " + outLine);
                                         if (!errLine.isEmpty()) MW_show_log("[WARP] stderr: " + errLine);
                                     }
                                 }
                                 if (elevatedUpProcess == process) {
                                     elevatedUpProcess = nullptr;
                                     elevatedUpExited = true;
                                     elevatedUpPid = 0;
                                     // The helper owned the tunnel, so its exit means there is
                                     // nothing left to wait for. Do not probe Status() here to
                                     // decide: this runs on the UI thread and Status() blocks in
                                     // runWarpPosix for up to 30s, which would freeze the very
                                     // event loop the admin password sheet needs.
                                     elevatedUpExternal = false;
                                 }
                                 process->deleteLater();
                             });
        }

        void trackElevatedUp(QProcess *process) {
            if (process == nullptr) return;
            if (QThread::currentThread() == qApp->thread()) {
                trackElevatedUpOnUi(process);
                return;
            }
            QMetaObject::invokeMethod(qApp, [process] { trackElevatedUpOnUi(process); },
                                      Qt::BlockingQueuedConnection);
        }

        bool elevatedUpAlreadyRunning() {
            const auto pid = elevatedUpPid.load();
            if (pidAlive(pid)) return true;
            if (pid > 0) elevatedUpPid = 0;
            return elevatedUpExternal.load() && !elevatedUpExited.load();
        }

        QString elevatedUpFailure(bool waitForResult = false) {
            QMutexLocker lock(&elevatedUpResultMutex);
            if (waitForResult && elevatedUpExitError.isEmpty()) {
                elevatedUpResultReady.wait(&elevatedUpResultMutex, 500);
            }
            return elevatedUpExitError.isEmpty()
                ? QStringLiteral("WARP helper exited before becoming ready")
                : elevatedUpExitError;
        }

        // Poll status without treating "status" process failure as Alive.
        // Fail fast if the tracked elevated pid already exited (password cancel).
        // Never touch QProcess from this thread.
        // Stop waiting on our elevated `up` and take its tunnel down with it.
        // A helper that was already running owns its own recovery lifecycle, so
        // neither a readiness timeout nor a user abort may terminate it or
        // trigger fresh elevation.
        void abandonElevatedUp() {
            const auto pid = elevatedUpPid.exchange(0);
            terminatePid(pid);
            if (QThread::currentThread() != qApp->thread()) {
                QMetaObject::invokeMethod(qApp, [] {
                    if (elevatedUpProcess != nullptr
                        && elevatedUpProcess->state() != QProcess::NotRunning) {
                        elevatedUpProcess->terminate();
                    }
                }, Qt::QueuedConnection);
            } else if (elevatedUpProcess != nullptr
                       && elevatedUpProcess->state() != QProcess::NotRunning) {
                elevatedUpProcess->terminate();
            }
            elevatedUpExternal = false;
        }

        bool waitAlive(QString *error, const char *timeoutMsg, bool existingHelper = false,
                       const std::function<bool()> &canceled = {}) {
            for (int i = 0; i < 120; ++i) { // ~60s
                const auto status = WarpProcess::Status();
                switch (WarpWaitStep(status, canceled && canceled(), existingHelper)) {
                    case WarpWaitAction::Ready:
                        elevatedUpExternal = false;
                        elevatedUpExited = false;
                        return true;
                    case WarpWaitAction::Abort:
                        // The tunnel never came up, so leaving the helper alive
                        // would land WARP on seconds after the user declined it.
                        if (!existingHelper) abandonElevatedUp();
                        if (error) *error = QStringLiteral("WARP start canceled");
                        return false;
                    case WarpWaitAction::HelperGone:
                        if (error) *error = QStringLiteral("WARP helper exited while recovering");
                        return false;
                    case WarpWaitAction::KeepWaiting:
                        break;
                }
                if (elevatedUpExited.exchange(false)) {
                    elevatedUpExternal = false;
                    if (error) *error = elevatedUpFailure();
                    return false;
                }
                const auto pid = elevatedUpPid.load();
                if (pid > 0 && !pidAlive(pid)) {
                    elevatedUpPid = 0;
                    elevatedUpExternal = false;
                    if (error) *error = elevatedUpFailure(true);
                    return false;
                }
                QThread::msleep(500);
            }
            if (!existingHelper) abandonElevatedUp();
            if (error) *error = timeoutMsg;
            return false;
        }

        struct ElevatedWaitResult {
            QMutex mu;
            QWaitCondition cv;
            bool finished = false;
            int exitCode = -1;
            QString stdoutText;
            QString stderrText;
            QString errorString;
        };

        // macOS auth UI for "with administrator privileges" is unreliable from
        // non-main threads. Create/start osascript only on the GUI thread.
        // Workers wait via pid / finished signal — never call QProcess methods.
        // Password sheet needs a free event loop: never waitForFinished on UI
        // while a worker holds BlockingQueuedConnection into the UI thread.
        bool startElevated(const QString &command, QString *output, QString *error, bool waitFinish) {
#ifdef Q_OS_MACOS
            if (command == "up" && elevatedUpAlreadyRunning()) {
                if (output) *output = "up already running";
                return true;
            }

            // Every elevated command needs the env inlined, not just "up": the root
            // shell osascript spawns inherits nothing from setProcessEnvironment, so
            // a bare "down" resolves a different data dir and cannot read state.json.
            const auto shell = command == "up"
                ? elevatedUpShellFragment()
                : BuildElevatedShell(warpPath(), command, warpDataDir(), warpLogPath(),
                                     getuid(), getgid());
            auto scriptShell = shell;
            scriptShell.replace("\\", "\\\\").replace("\"", "\\\"");
            // Prefer quoted form used by standalone GUI: do shell script "cmd" ...
            // keeps AppleScript parsing simple and shows the password sheet reliably.
            const auto script = QString("do shell script \"%1\" with administrator privileges").arg(scriptShell);
            const auto env = warpEnvironment();

            if (!waitFinish) {
                // Foreground warp-client owns the tunnel until down/SIGTERM.
                if (!prepareWarpLog(error)) return false;
                elevatedUpExited = false;
                QString startError;
                bool started = false;
                const auto startOnUi = [&] {
                    auto *process = new QProcess(qApp);
                    process->setProcessEnvironment(env);
                    process->start(QStringLiteral("/usr/bin/osascript"), {QStringLiteral("-e"), script});
                    started = process->waitForStarted(kOsascriptStartTimeoutMs);
                    if (!started) {
                        startError = process->errorString();
                        process->deleteLater();
                        return;
                    }
                    trackElevatedUpOnUi(process);
                };
                if (QThread::currentThread() == qApp->thread()) startOnUi();
                else QMetaObject::invokeMethod(qApp, startOnUi, Qt::BlockingQueuedConnection);
                if (!started) {
                    if (error) *error = startError.isEmpty() ? QStringLiteral("failed to start osascript") : startError;
                    return false;
                }
                return true;
            }

            // waitFinish elevated command (down): UI owns QProcess; worker waits on cv.
            // Always use the async finished path so the password sheet can run.
            auto result = std::make_shared<ElevatedWaitResult>();
            QString startError;
            bool started = false;
            qint64 childPid = 0;
            const auto startOnUi = [&] {
                auto *process = new QProcess(qApp);
                process->setProcessEnvironment(env);
                process->start(QStringLiteral("/usr/bin/osascript"), {QStringLiteral("-e"), script});
                started = process->waitForStarted(kOsascriptStartTimeoutMs);
                if (!started) {
                    startError = process->errorString();
                    process->deleteLater();
                    return;
                }
                childPid = process->processId();
                QObject::connect(process, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished),
                                 process, [process, result](int code, QProcess::ExitStatus) {
                                     {
                                         QMutexLocker lock(&result->mu);
                                         result->exitCode = code;
                                         result->stdoutText = QString::fromLocal8Bit(process->readAllStandardOutput());
                                         result->stderrText = QString::fromLocal8Bit(process->readAllStandardError()).trimmed();
                                         result->errorString = process->errorString();
                                         result->finished = true;
                                         result->cv.wakeAll();
                                     }
                                     process->deleteLater();
                                 });
            };

            if (QThread::currentThread() == qApp->thread()) {
                // On UI: start, then spin a local wait with processEvents so the sheet shows.
                startOnUi();
                if (!started) {
                    if (error) *error = startError.isEmpty() ? QStringLiteral("failed to start osascript") : startError;
                    return false;
                }
                for (int i = 0; i < 1200; ++i) { // ~120s
                    {
                        QMutexLocker lock(&result->mu);
                        if (result->finished) break;
                    }
                    qApp->processEvents(QEventLoop::AllEvents, 100);
                    QThread::msleep(100);
                }
            } else {
                QMetaObject::invokeMethod(qApp, startOnUi, Qt::BlockingQueuedConnection);
                if (!started) {
                    if (error) *error = startError.isEmpty() ? QStringLiteral("failed to start osascript") : startError;
                    return false;
                }
                QMutexLocker lock(&result->mu);
                if (!result->finished && !result->cv.wait(&result->mu, 120000)) {
                    terminatePid(childPid);
                    QMetaObject::invokeMethod(qApp, [childPid] {
                        for (auto *obj : qApp->findChildren<QProcess *>()) {
                            if (obj->processId() == childPid) {
                                obj->kill();
                                break;
                            }
                        }
                    }, Qt::QueuedConnection);
                    if (!result->finished) result->cv.wait(&result->mu, 3000);
                    if (!result->finished) {
                        if (error) *error = QStringLiteral("admin password prompt timed out");
                        return false;
                    }
                }
            }

            {
                QMutexLocker lock(&result->mu);
                if (!result->finished) {
                    terminatePid(childPid);
                    if (error) *error = QStringLiteral("admin password prompt timed out");
                    return false;
                }
                if (output) *output = result->stdoutText;
                const auto ok = result->exitCode == 0;
                if (!ok && error) {
                    auto err = result->stderrText;
                    if (err.isEmpty()) err = QStringLiteral("administrator authentication failed or was cancelled");
                    *error = err;
                }
                return ok;
            }
#else
            Q_UNUSED(command);
            Q_UNUSED(output);
            if (error) *error = "elevated WARP control is only implemented on macOS";
            return false;
#endif
        }
    }

    WarpRuntimeInfo WarpProcess::RuntimeInfo() {
        QString output, error;
        if (!Run("status", &output, &error, false, true)) return {};
        return ParseWarpRuntimeInfo(output);
    }

    WarpStatus WarpProcess::Status() {
        return RuntimeInfo().status;
    }

    QString WarpProcess::ElevatedUpShell(QString *error) {
        if (!prepareWarpLog(error)) return {};
        return elevatedUpShellFragment();
    }

    void WarpProcess::NoteElevatedUpLaunched(QProcess *process) {
        elevatedUpExternal = true;
        if (process != nullptr) trackElevatedUp(process);
    }

    bool WarpProcess::Run(const QString &command, QString *output, QString *error, bool elevated, bool waitFinish) {
        if (command != "status" && command != "up" && command != "down") {
            if (error) *error = "unsupported warp-client command";
            return false;
        }

        const QFileInfo binary(warpPath());
        if (!binaryTrusted(binary, error)) return false;

        QString captured;
        auto *outputTarget = output;
        if (command != "status" && outputTarget == nullptr && waitFinish) {
            outputTarget = &captured;
        }

        bool ok = false;
        if (elevated) {
            ok = startElevated(command, outputTarget, error, waitFinish);
        } else {
            // Non-elevated: never use QProcess from workers (macOS CFSocket crash).
            if (!waitFinish) {
                // Only elevated "up" is fire-and-forget; keep a safe no-op path.
                if (error) *error = "non-elevated background warp start is unsupported";
                return false;
            }
            ok = runWarpPosix(command, outputTarget, error, 30000);
        }
        if (!captured.isEmpty()) forwardWarpText(captured, true);
        return ok;
    }

    bool WarpProcess::Up(QString *error, std::function<bool()> canceled) {
        const auto status = Status();
        if (status == WarpStatus::Alive) {
            elevatedUpExternal = false;
            return true;
        }
        // Checked before elevating: a cancel that lands while the previous phase
        // was still running must not raise a password prompt the user cannot
        // answer for a start they already abandoned.
        if (canceled && canceled()) {
            if (error) *error = QStringLiteral("WARP start canceled");
            return false;
        }
        if (status == WarpStatus::Recovering) {
            return waitAlive(error, "WARP did not recover before the readiness timeout", true, canceled);
        }
        // Stale cleanup is done inside elevated warp-client up so the user only
        // gets one admin password prompt for a follow/independent start.
        // If Tun already launched elevated up in the same auth, just wait.
        if (elevatedUpExternal || elevatedUpAlreadyRunning()) {
            const bool ok = waitAlive(error, "WARP did not become ready (password prompt timed out or tunnel failed)",
                                      false, canceled);
            if (ok) elevatedUpExternal = false;
            return ok;
        }
        if (!Run("up", nullptr, error, true, false)) return false;
        // Password prompt + route setup can exceed a few seconds; wait longer
        // before declaring failure. Do not auto-down here: that would force a
        // second password prompt on top of the failed first attempt.
        return waitAlive(error, "WARP did not become ready (password prompt timed out or tunnel failed)",
                         false, canceled);
    }

    bool WarpProcess::Down(QString *error) {
        if (Status() == WarpStatus::Down) return true;
        // down needs elevation for route restore; one admin prompt is expected here.
        if (!Run("down", nullptr, error, true, true)) return false;
        for (int i = 0; i < 40; ++i) {
            if (Status() == WarpStatus::Down) return true;
            QThread::msleep(100);
        }
        if (error) *error = "WARP did not stop";
        return false;
    }

    CoreProcess::CoreProcess(const QString &core_path, const QString &socketName, bool debugMode)
        : m_program(core_path), m_socketName(socketName), m_debugMode(debugMode) {}

    bool CoreProcess::Start(qint64 *pid) {
        if (!QFileInfo::exists(m_program) || !QFileInfo(m_program).isExecutable()) {
            qWarning() << "ThroneCore missing or not executable:" << m_program;
            return false;
        }
        auto env = QProcessEnvironment::systemEnvironment();
        env.insert("THRONE_CORE_SOCKET", m_socketName);
        if (m_debugMode) env.insert("THRONE_CORE_DEBUG", "1");
        env.insert("XRAY_LOCATION_ASSET", Configs::GetBasePath());

        QProcess process;
        process.setProgram(m_program);
        process.setProcessEnvironment(env);
        process.setWorkingDirectory(QApplication::applicationDirPath());
        process.setProcessChannelMode(QProcess::MergedChannels);
        process.setStandardOutputFile(Configs::GetBasePath() + "/core.log", QIODevice::Append);
        return process.startDetached(pid);
    }

    bool CoreProcess::Kill(qint64 pid) {
        if (pid <= 0) return true;

#ifdef Q_OS_WIN
        HANDLE process = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, FALSE, static_cast<DWORD>(pid));
        if (process == nullptr) return GetLastError() == ERROR_INVALID_PARAMETER;
        const bool stopped = TerminateProcess(process, 0) && WaitForSingleObject(process, 1000) == WAIT_OBJECT_0;
        CloseHandle(process);
#else
        if (::kill(static_cast<pid_t>(pid), SIGTERM) != 0 && errno != ESRCH) return false;
        bool stopped = false;
        for (int attempt = 0; attempt < 20; ++attempt) {
            if (::kill(static_cast<pid_t>(pid), 0) != 0 && errno == ESRCH) {
                stopped = true;
                break;
            }
            QThread::msleep(25);
        }
        if (!stopped) stopped = ::kill(static_cast<pid_t>(pid), SIGKILL) == 0 || errno == ESRCH;
#endif
        return stopped;
    }

    bool CoreProcess::IsAlive(qint64 pid) {
        if (pid <= 0) return false;
#ifdef Q_OS_WIN
        HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, static_cast<DWORD>(pid));
        if (process == nullptr) return false;
        DWORD code = 0;
        const bool alive = GetExitCodeProcess(process, &code) && code == STILL_ACTIVE;
        CloseHandle(process);
        return alive;
#else
        return ::kill(static_cast<pid_t>(pid), 0) == 0 || errno != ESRCH;
#endif
    }

} // namespace Configs_sys
