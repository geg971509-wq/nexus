#include "include/sys/macos/MacOS.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QIODevice>
#include <QObject>
#include <QProcess>
#include <QStandardPaths>
#include <QString>
#include <QThread>
#include <QUuid>
#include <sys/mount.h>
#include <sys/stat.h>
#include <unistd.h>

bool Mac_Core_Path_Supports_Setuid(const QString &path) {
    struct statfs filesystem {};
    const auto nativePath = QFile::encodeName(path);
    return statfs(nativePath.constData(), &filesystem) == 0
        && !(filesystem.f_flags & (MNT_NOSUID | MNT_IGNORE_OWNERSHIP));
}

static bool pathHasSetuid(const QString &path) {
    struct stat fileInfo {};
    if (stat(QFile::encodeName(path).constData(), &fileInfo) != 0) return false;
    return (fileInfo.st_mode & S_ISUID) != 0;
}

// Private dir + random token; reject world-writable/wrong-owner/wrong-content markers.
static bool markerIsOurs(const QString &markerPath, const QByteArray &token) {
    struct stat st {};
    if (stat(QFile::encodeName(markerPath).constData(), &st) != 0) return false;
    if (!S_ISREG(st.st_mode)) return false;
    if (st.st_uid != getuid()) return false;
    if ((st.st_mode & 077) != 0) return false; // group/other must have no access
    QFile f(markerPath);
    if (!f.open(QIODevice::ReadOnly)) return false;
    return f.readAll() == token;
}

int Mac_Set_Core_Permissions(const QString &path, const QString &postCommand, QProcess **elevatedProcess) {
    if (elevatedProcess) *elevatedProcess = nullptr;
    // Fixed core path + optional post command only; no free-form user input.
    const auto quotedPath = QString("'") + QString(path).replace("'", "'\\''") + "'";
    // Marker lives under the user runtime dir (not /tmp) and carries a random token so
    // a pre-existing setuid bit or a world-writable spoof cannot skip the password.
    const QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    QDir().mkpath(runtime.isEmpty() ? QDir::tempPath() : runtime);
    const QString markerDir = runtime.isEmpty() ? QDir::tempPath() : runtime;
    const QByteArray token = QUuid::createUuid().toByteArray(QUuid::WithoutBraces);
    const auto marker = QString("%1/throne-setuid-ready-%2")
                            .arg(markerDir)
                            .arg(QCoreApplication::applicationPid());
    const auto quotedMarker = QString("'") + QString(marker).replace("'", "'\\''") + "'";
    const auto quotedToken = QString("'") + QString::fromLatin1(token).replace("'", "'\\''") + "'";
    QFile::remove(marker);
    // Build shell without multi-arg %N placeholders (Qt re-scans replacements).
    // umask-safe write: printf token, chmod 600, chown to the real user.
    auto shell = QString("/usr/sbin/chown root:wheel %1 && /bin/chmod u+s %1").arg(quotedPath);
    shell += QString(" && /usr/bin/printf %1 > %2 && /bin/chmod 600 %2 && /usr/sbin/chown %3 %2")
                 .arg(quotedToken, quotedMarker, QString::number(getuid()));
    if (!postCommand.isEmpty()) {
        // Keep the elevated shell alive with warp-client up after setuid succeeds.
        shell += " && " + postCommand;
    }
    auto scriptShell = shell;
    scriptShell.replace("\\", "\\\\").replace("\"", "\\\"");
    const auto script = QString("do shell script \"%1\" with administrator privileges").arg(scriptShell);

    auto *process = new QProcess;
    process->start("/usr/bin/osascript", {"-e", script});
    if (!process->waitForStarted(kOsascriptStartTimeoutMs)) {
        process->deleteLater();
        return -1;
    }

    if (!postCommand.isEmpty()) {
        for (int i = 0; i < 120; ++i) { // ~60s
            if (process->state() == QProcess::NotRunning) {
                const auto code = process->exitCode();
                QFile::remove(marker);
                process->deleteLater();
                return code == 0 ? -1 : code;
            }
            // Marker means password accepted + setuid done; osascript should still be
            // in foreground warp-client up. Hand process to WarpProcess for waitAlive.
            if (markerIsOurs(marker, token) && pathHasSetuid(path)) {
                QFile::remove(marker);
                if (elevatedProcess) *elevatedProcess = process;
                else process->deleteLater();
                return 0;
            }
            QThread::msleep(500);
        }
        process->kill();
        QFile::remove(marker);
        process->deleteLater();
        return -1;
    }

    if (!process->waitForFinished(-1)) {
        QFile::remove(marker);
        process->deleteLater();
        return -1;
    }
    const auto code = process->exitCode();
    QFile::remove(marker);
    process->deleteLater();
    return code;
}
