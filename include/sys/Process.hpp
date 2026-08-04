#pragma once

#include <QtGlobal>
#include <QProcess>
#include <QString>
#include <QRegularExpression>

#include <functional>

namespace Configs_sys {
    enum class WarpStatus { Down, Alive, Recovering, Stale, Unknown };

    struct WarpRuntimeInfo {
        WarpStatus status = WarpStatus::Unknown;
        QString transport;
        // Underlay tunnel device. Config generation pins it as sing-box's
        // route.default_interface so proxy egress descends through WARP instead
        // of being auto-detected onto the physical interface.
        QString interfaceName;
    };

    inline WarpRuntimeInfo ParseWarpRuntimeInfo(const QString &output) {
        WarpRuntimeInfo info;
        // Classify on the status line itself, not on a substring found anywhere in
        // the output. The helper's error status embeds a filesystem path, and a path
        // is free to contain "status: down" verbatim -- a substring match would let
        // an unreadable state file be read as a confirmed-down tunnel, which is
        // exactly the misclassification WarpProcess::Down() early-returns on.
        //
        // Taking the FIRST such line is what makes that safe, not any quoting on the
        // producer side: a real newline inside the payload cannot promote a forged
        // line ahead of the genuine one. That holds only while the helper emits its
        // status line before anything else on the merged stdout/stderr pipe, so a
        // stray write ordered ahead of it would defeat this.
        QString statusLine;
        for (const auto &line : output.split('\n')) {
            const auto trimmed = line.trimmed();
            if (trimmed.startsWith("status:")) {
                statusLine = trimmed;
                break;
            }
        }
        if (statusLine.startsWith("status: down")) {
            info.status = WarpStatus::Down;
            return info;
        }
        if (!statusLine.startsWith("status: interface=")) return info;
        // Parsed before the alive gate: a stale tunnel still names its device,
        // and callers use the name to decide what to tear down.
        const auto ifaceMatch = QRegularExpression("\\binterface=([^\\s]+)").match(statusLine);
        if (ifaceMatch.hasMatch()) info.interfaceName = ifaceMatch.captured(1);
        const auto aliveMatch = QRegularExpression("\\balive=(true|false)\\b").match(statusLine);
        if (!aliveMatch.hasMatch()) return info;
        if (aliveMatch.captured(1) == "false") {
            info.status = WarpStatus::Stale;
            return info;
        }

        const auto transportMatch = QRegularExpression("\\btransport=([^\\s]+)").match(statusLine);
        if (transportMatch.hasMatch()) info.transport = transportMatch.captured(1);

        const auto healthMatch = QRegularExpression("\\bhealth=([^\\s]+)").match(statusLine);
        if (!healthMatch.hasMatch()) {
            info.status = statusLine.contains(QRegularExpression("\\bhealth="))
                ? WarpStatus::Unknown
                : WarpStatus::Alive;
            return info;
        }
        const auto health = healthMatch.captured(1);
        if (health == "healthy") info.status = WarpStatus::Alive;
        else if (health == "starting" || health == "recovering") info.status = WarpStatus::Recovering;
        return info;
    }

    inline WarpStatus ParseWarpStatusOutput(const QString &output) {
        return ParseWarpRuntimeInfo(output).status;
    }

    // Pure: what config generation may pin as route.default_interface.
    // Alive/Recovering only — Stale/Down/Unknown must not bind dials to a dead device.
    // Callers supply a cached RuntimeInfo; never probe from the UI thread.
    inline QString WarpUnderlayInterfaceForConfig(bool tunEnabled, bool enableWarp,
                                                 WarpStatus status, const QString &interfaceName) {
        if (!tunEnabled || !enableWarp) return {};
        if (status != WarpStatus::Alive && status != WarpStatus::Recovering) return {};
        return interfaceName;
    }

    // Pure: whether the 2s timer should fork warp-client status.
    // enable_warp on → always. Pinned underlay → always (must notice device death).
    // Otherwise only while lastKnown is not Down: Unknown at startup gets one
    // probe, then stops; Alive with enable_warp off still watches for teardown.
    // Does not discover a brand-new external tunnel while enable_warp is off.
    inline bool ShouldPollWarpStatus(bool enableWarp, WarpStatus lastKnown,
                                     bool hasPinnedUnderlay = false) {
        if (enableWarp || hasPinnedUnderlay) return true;
        return lastKnown != WarpStatus::Down;
    }

    enum class WarpWaitAction { Ready, Abort, HelperGone, KeepWaiting };

    // Pure: one iteration of the WARP readiness poll.
    // Alive outranks canceled so a tunnel that did come up is never abandoned
    // with its routes installed while the UI reports WARP off.
    // existingHelper means another auth already launched `up`: that helper owns
    // its own recovery, so Down/Stale is it giving up rather than a slow start.
    inline WarpWaitAction WarpWaitStep(WarpStatus status, bool canceled, bool existingHelper) {
        if (status == WarpStatus::Alive) return WarpWaitAction::Ready;
        if (canceled) return WarpWaitAction::Abort;
        if (existingHelper && (status == WarpStatus::Down || status == WarpStatus::Stale)) {
            return WarpWaitAction::HelperGone;
        }
        return WarpWaitAction::KeepWaiting;
    }

    inline QString WarpShellQuote(QString value) {
        return QString("'") + value.replace("'", "'\\''") + "'";
    }

    inline QString WarpDataDirIn(const QString &base) { return base + "/warp-client"; }

    // Deliberately one level above the data directory. The helper runs as root and
    // Go's EnsureDir chowns the data directory back to the invoking user, but a
    // directory left root-owned by an older build makes the unelevated GUI's own
    // QFile::open on the log fail with EACCES -- and that check runs before
    // osascript, so the app could never obtain the root it needs to repair itself.
    // The base directory is always user-owned, so the log is always writable and
    // EnsureDir gets its chance.
    inline QString WarpLogPathIn(const QString &base) { return base + "/warp.log"; }

    // osascript's `do shell script ... with administrator privileges` spawns a
    // fresh root shell that inherits nothing from QProcess::setProcessEnvironment,
    // so every variable has to be inlined into the command itself.
    inline QString WarpEnvPrefix(const QString &dataDir, const QString &logPath, uint uid, uint gid) {
        return QString("env THRONE_WARP_DATA_DIR=%1 THRONE_WARP_OWNER_UID=%2 THRONE_WARP_OWNER_GID=%3 THRONE_WARP_LOG_PATH=%4")
            .arg(WarpShellQuote(dataDir),
                 WarpShellQuote(QString::number(uid)),
                 WarpShellQuote(QString::number(gid)),
                 WarpShellQuote(logPath));
    }

    // No mkdir/chown prologue on purpose. Those would run as root before Go's
    // Lstat symlink rejection in EnsureDir, so a symlink pre-planted at the data
    // directory by anything running as this user would turn elevation into a root
    // file-creation primitive. Directory creation and ownership repair belong to
    // EnsureDir, which checks for symlinks first.
    inline QString BuildElevatedShell(const QString &binary, const QString &command,
                                      const QString &dataDir, const QString &logPath,
                                      uint uid, uint gid) {
        return QString("%1 %2 %3")
            .arg(WarpEnvPrefix(dataDir, logPath, uid, gid),
                 WarpShellQuote(binary),
                 WarpShellQuote(command));
    }

    // Non-package builds leave the base path at the app dir, where the data dir
    // name would resolve onto the warp-client binary itself (mkdir -> EEXIST,
    // open -> ENOTDIR).
    inline bool WarpDataDirCollides(const QString &dataDir, const QString &binaryPath) {
        return dataDir == binaryPath;
    }

    inline QString FormatWarpExitError(const QString &stderrText, const QString &stdoutText, int exitCode) {
        auto detail = stderrText.trimmed();
        if (detail.isEmpty()) detail = stdoutText.trimmed();
        return detail.isEmpty()
            ? QStringLiteral("WARP helper exited with code %1").arg(exitCode)
            : detail;
    }

    class WarpProcess {
    public:
        static WarpRuntimeInfo RuntimeInfo();
        static WarpStatus Status();
        // canceled is polled during the readiness wait so a user abort does not
        // have to sit out the full timeout. It never tears an already-live
        // tunnel back down; the caller owns that decision.
        static bool Up(QString *error = nullptr, std::function<bool()> canceled = {});
        static bool Down(QString *error = nullptr);
        // Shell fragment for elevating warp-client up inside another admin script
        // (e.g. combined with core setuid so Tun+WARP needs one password).
        static QString ElevatedUpShell(QString *error = nullptr);
        // Mark that elevated up was already launched (e.g. inside setuid auth).
        // Pass the still-running elevated QProcess so waitAlive can fail-fast on cancel.
        static void NoteElevatedUpLaunched(QProcess *process = nullptr);

    private:
        // waitFinish=false is only for "up", which stays foreground until down.
        static bool Run(const QString &command, QString *output, QString *error, bool elevated, bool waitFinish);
    };

    // Pure helpers for detached-core pre-IPC recovery (testable without QProcess).
    // Relaunch only when the watched child died before IPC and the launch token still matches.
    inline bool ShouldRelaunchPreIpcCore(bool prepare_exit,
                                        bool core_running,
                                        quint64 current_generation,
                                        quint64 watch_generation,
                                        qint64 watch_pid,
                                        qint64 current_launched_pid,
                                        bool pid_alive) {
        if (prepare_exit || core_running) return false;
        if (watch_generation != current_generation) return false;
        if (watch_pid <= 0 || watch_pid != current_launched_pid) return false;
        return !pid_alive;
    }

    inline bool ShouldContinuePreIpcWatch(bool prepare_exit,
                                          bool core_running,
                                          quint64 current_generation,
                                          quint64 watch_generation,
                                          qint64 watch_pid,
                                          qint64 current_launched_pid,
                                          bool pid_alive) {
        if (prepare_exit || core_running) return false;
        if (watch_generation != current_generation) return false;
        if (watch_pid <= 0 || watch_pid != current_launched_pid) return false;
        return pid_alive;
    }

    // Matches upstream: reject relaunches closer than 10s to avoid crash loops.
    inline bool AllowPreIpcRelaunch(qint64 now_ms, qint64 last_relaunch_ms, qint64 min_interval_ms = 10 * 1000) {
        if (last_relaunch_ms <= 0) return true;
        return (now_ms - last_relaunch_ms) >= min_interval_ms;
    }

    class CoreProcess {
    public:
        CoreProcess(const QString &core_path, const QString &socketName, bool debugMode);

        bool Start(qint64 *pid = nullptr);

        bool Kill(qint64 pid);

        static bool IsAlive(qint64 pid);

        int start_profile_when_core_is_up = -1;

    private:
        QString m_program;
        QString m_socketName;
        bool m_debugMode = false;
    };
} // namespace Configs_sys
