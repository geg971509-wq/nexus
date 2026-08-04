// WARP underlay: toolbar/menu toggle, runtime status polling, the button and
// tray-icon presentation, and the preflight used by profile start.
#include "include/ui/mainwindow.h"

#include "include/sys/Process.hpp"
#include "include/ui/setting/Icon.hpp"
#include "include/database/SettingsRepo.h"

#include <QMessageBox>
#include <QPainter>
#include <QSignalBlocker>
#include <QStyle>

#include <chrono>
#include <thread>

QString MainWindow::cachedWarpUnderlayForConfig() const {
    return Configs_sys::WarpUnderlayInterfaceForConfig(
        Configs::dataManager->settingsRepo->spmode_vpn,
        Configs::dataManager->settingsRepo->enable_warp,
        warpRuntimeStatus,
        warpInterfaceName);
}

void MainWindow::refreshWarpRuntimeStatus() {
    if (!acceptingOperations_.load() || warpOpBusy.load()) return;
    if (!Configs_sys::ShouldPollWarpStatus(
            Configs::dataManager->settingsRepo->enable_warp,
            warpRuntimeStatus,
            !warpInterfaceInRunningConfig.isEmpty())) {
        return;
    }
    bool expected = false;
    if (!warpStatusPollInFlight.compare_exchange_strong(expected, true)) return;

    const auto generation = warpStatusGeneration.load();
    operationCallPool->start([this, generation] {
        const auto info = Configs_sys::WarpProcess::RuntimeInfo();
        runOnUiThread([this, generation, info] {
            warpStatusPollInFlight = false;
            if (!acceptingOperations_.load()
                || generation != warpStatusGeneration.load()
                || warpOpBusy.load()) return;

            warpRuntimeStatus = info.status;
            warpTransport = info.transport;
            warpInterfaceName = info.interfaceName;
            refreshWarpButton();
            icon_status = -1;
            refresh_status();

            // The running config may have pinned WARP's device as
            // route.default_interface, which binds every sing-box dial to it. If
            // that device is gone or renamed, the core is dialing into nothing and
            // will stay broken until the config is rebuilt. Only setWarpEnabled
            // used to rebuild, which covers a deliberate toggle but not WARP dying
            // on its own -- and pinning made that failure worse than before, since
            // egress used to just fall back to the physical interface.
            if (!warpInterfaceInRunningConfig.isEmpty()
                && info.interfaceName != warpInterfaceInRunningConfig
                && Configs::dataManager->settingsRepo->started_id >= 0) {
                MW_show_log(tr("[WARP] Underlay device %1 is gone (now %2); rebuilding core config")
                                .arg(warpInterfaceInRunningConfig,
                                     info.interfaceName.isEmpty() ? tr("none") : info.interfaceName));
                warpInterfaceInRunningConfig.clear();
                profile_start(Configs::dataManager->settingsRepo->started_id);
            }
        }, true);
    });
}

void MainWindow::refreshWarpButton() {
    if (!ui || !ui->toolButton_WARP) return;
    const bool busy = warpOpBusy.load();
    const bool desiredOn = warpDesiredOn.load();
    const bool recovering = !busy && warpRuntimeStatus == Configs_sys::WarpStatus::Recovering;
    const bool on = busy ? desiredOn
                         : warpRuntimeStatus == Configs_sys::WarpStatus::Alive || recovering;
    QSignalBlocker blocker(ui->toolButton_WARP);
    ui->toolButton_WARP->setChecked(on);
    ui->toolButton_WARP->setEnabled(!busy || recovering);
    if (ui->toolButton_WARP->property("recovering").toBool() != recovering) {
        ui->toolButton_WARP->setProperty("recovering", recovering);
        ui->toolButton_WARP->style()->unpolish(ui->toolButton_WARP);
        ui->toolButton_WARP->style()->polish(ui->toolButton_WARP);
    }
    if (busy) {
        ui->toolButton_WARP->setToolTip(desiredOn
                                            ? tr("Starting system WARP tunnel…")
                                            : tr("Stopping system WARP tunnel…"));
    } else if (recovering) {
        ui->toolButton_WARP->setToolTip(
            warpTransport.isEmpty()
                ? tr("System WARP is reconnecting automatically. Click to turn off the underlay.")
                : tr("System WARP is reconnecting over %1. Click to turn off the underlay.")
                      .arg(warpTransport.toUpper()));
    } else if (on) {
        ui->toolButton_WARP->setToolTip(
            warpTransport.isEmpty()
                ? tr("System WARP underlay is active. Click to turn it off.")
                : tr("System WARP underlay is active over %1. Click to turn it off.")
                      .arg(warpTransport.toUpper()));
    } else {
        QString tip = tr("WARP is off. Click to turn on the system tunnel.");
        if (warpRuntimeStatus == Configs_sys::WarpStatus::Stale) {
            tip = tr("WARP looks stale. Click to clean up and turn it on.");
        } else if (warpRuntimeStatus == Configs_sys::WarpStatus::Unknown) {
            tip = tr("WARP status unknown. Click to try turning it on.");
        }
        ui->toolButton_WARP->setToolTip(tip);
    }
}

bool MainWindow::acquireWarpOp(int waitMs) {
    // Down must not be skipped when busy: wait briefly so cancel/stop still tears tunnel.
    const int step = 50;
    for (int waited = 0; waited <= waitMs; waited += step) {
        bool expected = false;
        if (warpOpBusy.compare_exchange_strong(expected, true)) {
            ++warpStatusGeneration;
            return true;
        }
        if (waited == waitMs) break;
        std::this_thread::sleep_for(std::chrono::milliseconds(step));
    }
    return false;
}

void MainWindow::releaseWarpOp() {
    warpOpBusy = false;
}

bool MainWindow::askContinueWithoutWarp(const QString &error) {
    bool cont = false;
    const bool proxyAlreadyRunning = Configs::dataManager->settingsRepo->started_id >= 0;
    runOnUiThread([=, this, &cont] {
        const auto answer = QMessageBox::question(
            this,
            tr("WARP failed"),
            proxyAlreadyRunning
                ? tr("Failed to start WARP:\n%1\n\nKeep the proxy running without WARP?")
                      .arg(error.isEmpty() ? tr("unknown error") : error)
                : tr("Failed to start WARP:\n%1\n\nContinue starting the proxy without WARP?")
                      .arg(error.isEmpty() ? tr("unknown error") : error),
            QMessageBox::Yes | QMessageBox::No,
            QMessageBox::No);
        cont = answer == QMessageBox::Yes;
    }, true);
    return cont;
}

bool MainWindow::ensureWarpReady(bool *startedThisAttempt, QString *error,
                                 const std::function<bool()> &canceled) {
    if (startedThisAttempt) *startedThisAttempt = false;
    auto status = Configs_sys::WarpProcess::Status();
    if (status == Configs_sys::WarpStatus::Alive) return true;
    // Stale cleanup is inside elevated warp-client up (one password prompt).
    QString upError;
    if (!Configs_sys::WarpProcess::Up(&upError, canceled)) {
        if (error) *error = upError;
        return false;
    }
    status = Configs_sys::WarpProcess::Status();
    if (status != Configs_sys::WarpStatus::Alive) {
        if (error) *error = upError.isEmpty() ? QStringLiteral("WARP did not become ready") : upError;
        return false;
    }
    // Toolbar owns tunnel lifecycle; proxy start only preflights.
    if (startedThisAttempt) *startedThisAttempt = true;
    return true;
}

bool MainWindow::setWarpEnabled(bool enable) {
    if (!acquireWarpOp(0)) {
        refreshWarpButton();
        QMessageBox::warning(this, tr("WARP"), tr("WARP operation already in progress"));
        return false;
    }
    warpDesiredOn = enable;
    refreshWarpButton();
    MW_show_log(enable ? tr("[WARP] Starting system underlay (admin password prompt should appear)...")
                       : tr("[WARP] Stopping system underlay (admin password prompt should appear)..."));
    ActivateWindow(this);
    operationCallPool->start([=, this] {
        QString error;
        bool ok = false;
        try {
            ok = enable ? Configs_sys::WarpProcess::Up(&error)
                        : Configs_sys::WarpProcess::Down(&error);
        } catch (...) {
            if (error.isEmpty()) error = QStringLiteral("WARP operation aborted");
        }
        const auto info = Configs_sys::WarpProcess::RuntimeInfo();
        runOnUiThread([this, ok, enable, error, info] {
            if (!acceptingOperations_.load()) {
                releaseWarpOp();
                return;
            }
            warpRuntimeStatus = info.status;
            warpTransport = info.transport;
            warpInterfaceName = info.interfaceName;
            const bool enabled = info.status == Configs_sys::WarpStatus::Alive
                              || info.status == Configs_sys::WarpStatus::Recovering;
            auto &settings = Configs::dataManager->settingsRepo;
            bool settingsChanged = false;
            if (ok && enabled == enable) {
                settingsChanged = settings->enable_warp != enable;
                settings->enable_warp = enable;
                settings->Save();
            }
            warpDesiredOn = settings->enable_warp;
            releaseWarpOp();
            refreshWarpButton();
            icon_status = -1;
            refresh_status();
            // With Tun on, the underlay changes what the core config must contain:
            // route.default_interface pins egress to WARP's device, and the Tun
            // route_address set is split so it shares no routing-table key with
            // WARP's. Both are decided at generation time, so a live core keeps the
            // stale shape until it is restarted. Without Tun nothing warp-dependent
            // is emitted, so a restart would only drop connections for nothing.
            if (settingsChanged && settings->spmode_vpn && settings->started_id >= 0) {
                MW_show_log(tr("[WARP] Restarting core to apply the new underlay routing"));
                profile_start(settings->started_id);
            }
            if (!ok || (enable && !enabled) || (!enable && enabled)) {
                MW_show_log(tr("[WARP] [Warn] %1 failed: %2")
                                .arg(enable ? tr("start") : tr("stop"),
                                     error.isEmpty() ? tr("unknown error") : error));
                QMessageBox::warning(this, tr("WARP"),
                                     error.isEmpty() ? tr("WARP operation failed") : error);
            } else {
                MW_show_log(enabled
                                ? tr("[WARP] System underlay is on (%1)").arg(warpTransport.isEmpty() ? tr("transport unknown") : warpTransport.toUpper())
                                : tr("[WARP] System underlay is off"));
            }
        }, true);
    });
    return true;
}

void MainWindow::updateWarpTraySpin(bool spinning) {
    if (!m_warpTraySpinTimer) {
        m_warpTraySpinTimer = new QTimer(this);
        connect(m_warpTraySpinTimer, &QTimer::timeout, this, [this] {
            if (!tray || icon_status != Icon::WARP) return;
            m_warpTrayAngle += 12;
            if (m_warpTrayAngle >= 360) m_warpTrayAngle -= 360;
            tray->setIcon(warpTrayIconAtAngle(m_warpTrayAngle));
        });
    }
    if (spinning) {
        if (m_warpTrayBase.isNull()) {
            m_warpTrayBase = Icon::GetTrayIcon(Icon::WARP).scaled(
                64, 64, Qt::KeepAspectRatio, Qt::SmoothTransformation);
        }
        if (!m_warpTraySpinTimer->isActive()) m_warpTraySpinTimer->start(80);
    } else if (m_warpTraySpinTimer->isActive()) {
        m_warpTraySpinTimer->stop();
        m_warpTrayAngle = 0;
    }
}

QIcon MainWindow::warpTrayIconAtAngle(qreal angle) const {
    if (m_warpTrayBase.isNull()) return {};
    QPixmap pm(m_warpTrayBase.size());
    pm.fill(Qt::transparent);
    QPainter p(&pm);
    p.setRenderHint(QPainter::SmoothPixmapTransform);
    p.translate(pm.width() / 2.0, pm.height() / 2.0);
    p.rotate(angle);
    p.translate(-pm.width() / 2.0, -pm.height() / 2.0);
    p.drawPixmap(0, 0, m_warpTrayBase);
    return QIcon(pm);
}
