#include "include/ui/mainwindow.h"

#include "include/stats/traffic/TrafficLooper.hpp"
#include "include/stats/traffic/TrafficStatsManager.hpp"
#include "include/stats/autoselector/AutoSelectorMonitor.hpp"
#include "include/configs/AutoSelectorPlan.h"
#include "include/api/RPC.h"
#include "include/ui/utils//MessageBoxTimer.h"
#include "3rdparty/qv2ray/v2/proxy/QvProxyConfigurator.hpp"

#include <QInputDialog>
#include <QPushButton>
#include <QDesktopServices>
#include <QMessageBox>
#include <QJsonDocument>
#include <QFile>
#include <QScopeGuard>
#include <QStyle>
#include <QRegularExpression>

#include "include/configs/generate.h"
#include "include/global/HTTPRequestHelper.hpp"
#include "include/configs/common/xrayStreamSetting.h"
#include "include/database/GroupsRepo.h"
#include "include/database/ProfilesRepo.h"

#include "include/sys/Process.hpp"

#include <algorithm>
#include <latch>

#include <memory>
#include <thread>
#include <chrono>

// rpc

using namespace API;

namespace {
    // How long the "no response, restart recommended" MessageBoxTimer waits
    // before auto-dismissing: profile start vs profile stop paths.
    static constexpr int kStartRestartHintTimeoutMs = 10000;
    static constexpr int kStopRestartHintTimeoutMs = 5000;

    // An auto selector has no server of its own to measure: whichever member
    // answers is the core's business and changes minute to minute, so a stored
    // result would be noise on a row that can never act on it. Group-wide tests
    // drop it instead of showing a number that means nothing.
    QList<int> withoutAutoSelectors(const QList<int>& profileIDs) {
        const auto selectors = Configs::dataManager->profilesRepo->GetProfileIdsByType("autoselector");
        if (selectors.isEmpty()) return profileIDs;
        const QSet<int> skip(selectors.begin(), selectors.end());
        QList<int> filtered;
        filtered.reserve(profileIDs.size());
        for (int id : profileIDs) {
            if (!skip.contains(id)) filtered << id;
        }
        return filtered;
    }
} // namespace

void MainWindow::setup_rpc(QLocalSocket *socket) {
    // The Client is constructed once at startup and never recreated; on core
    // restart we only swap the underlying connection, so worker threads holding
    // `defaultClient` never touch freed memory.
    defaultClient->Reconnect(socket);

    // Loopers run for the lifetime of the app, start only once
    if (!rpc_started) {
        rpc_started = true;
        Stats::trafficLooper->Start();
        Stats::connection_lister->Start();
        Stats::autoSelectorMonitor->Start();
    }
}

// Ranking is only worth the wait when the selector has more eligible members
// than it can run: below that everything is built anyway. Runs synchronously on
// the caller's worker thread; the caller is already off the UI thread.
// Measures only what has no result yet (plus anything explicitly stale) and
// re-ranks. Reusing the group's existing URL-test results is the point: a user
// who just tested the group must not trigger a second sweep behind their back.
void MainWindow::rank_auto_selector(const std::shared_ptr<Configs::Profile>& ent, const QList<int>& stale) {
    if (ent == nullptr || ent->type != "autoselector") return;

    const auto needed = Configs::AutoSelectorUnmeasuredCandidates(ent, stale);
    if (needed.isEmpty()) {
        const auto ranked = Configs::RerankAutoSelectorPool(ent);
        MW_show_log(tr("[Auto selector] Reusing existing test results; ranked %1 profiles.").arg(ranked.size()));
        return;
    }

    MW_show_log(tr("[Auto selector] Measuring %1 not-yet-tested profiles...").arg(needed.size()));
    // Marshalled: the busy-test prompt is modal, and a modal dialog may not be
    // built off the UI thread. Returns as soon as the sweep is queued.
    runOnUiThread([=, this] { urltest_current_group(needed); }, true);
    // The sweep holds speedtestOperation_ for its whole duration, so once the
    // gate is idle again every latency is in the database.
    while (speedtestOperation_.state() != Throne::OperationState::Idle) {
        if (!acceptingOperations_.load()) return; // exiting; nothing to rank into
        QThread::msleep(100);
    }

    const auto ranked = Configs::RerankAutoSelectorPool(ent);
    MW_show_log(tr("[Auto selector] Ranked %1 profiles.").arg(ranked.size()));
}

void MainWindow::on_subscription_group_changed(int gid, const QList<int>& disturbed) {
    if (gid < 0) return;
    const QSet<int> disturbedSet(disturbed.begin(), disturbed.end());
    int restartID = -1;

    for (int id : Configs::dataManager->profilesRepo->GetProfileIdsByType("autoselector")) {
        auto ent = Configs::dataManager->profilesRepo->GetProfile(id);
        if (ent == nullptr) continue;
        auto selector = ent->AutoSelector();
        if (selector == nullptr || selector->gid != gid) continue;

        // The ranked pool is only a prior — PlanAutoSelector already ignores
        // members that no longer resolve. Pruning it keeps the stored list from
        // growing a tail of dead ids across refreshes, and keeps lastBuilt
        // honest for the exhausted path, which re-tests it as known-stale.
        const auto gone = [](int memberID) {
            return Configs::dataManager->profilesRepo->GetProfile(memberID) == nullptr;
        };
        const auto prunedPool = selector->pool.removeIf(gone);
        const auto prunedBuilt = selector->lastBuilt.removeIf(gone);
        if (prunedPool > 0 || prunedBuilt > 0) Configs::dataManager->profilesRepo->Save(ent);

        // Only the running one holds a config that can go stale. A member it
        // never built changing is something the next build picks up by itself.
        if (running == nullptr || running->id != ent->id) continue;
        // A deleted member is already out of lastBuilt; a replaced one kept its
        // id, so it takes the disturbed set to spot.
        bool rebuild = prunedBuilt > 0;
        for (int memberID : selector->lastBuilt) {
            if (!disturbedSet.contains(memberID)) continue;
            rebuild = true;
            break;
        }
        if (rebuild) restartID = ent->id;
    }

    if (restartID < 0) return;
    MW_show_log(tr("[Auto selector] The subscription replaced profiles it was running on — rebuilding."));
    profile_start(restartID);
}

void MainWindow::on_auto_selector_exhausted(int profileID) {
    auto ent = Configs::dataManager->profilesRepo->GetProfile(profileID);
    if (ent == nullptr || running == nullptr || running->id != profileID) return;

    MW_show_log(tr("[Auto selector] Every running profile stopped working — rebuilding from the "
                   "next best candidates."));
    runOnNewThread([=, this] {
        // The members that just died carry stale results, so they are re-tested
        // even though they have one; everything else is only measured if it has
        // never been. The failures then sink and fresh candidates rise.
        QList<int> stale;
        if (auto selector = ent->AutoSelector(); selector != nullptr) stale = selector->lastBuilt;
        rank_auto_selector(ent, stale);
        runOnUiThread([=, this] {
            if (running == nullptr || running->id != profileID) return;
            profile_start(profileID);
        });
    });
}

void MainWindow::runURLTest(const QString& config, const QString& xrayConfig, const QStringList& xrayFullConfigs, bool useDefault, const QStringList& outboundTags, const QMap<QString, int>& tag2entID, int entID) {
    if (stopSpeedtest.load()) {
        MW_show_log(tr("Profile test aborted"));
        return;
    }

    libcore::TestReq req;
    for (const auto &item: outboundTags) {
        req.outbound_tags.push_back(item.toStdString());
    }
    req.config = config.toStdString();
    req.url = Configs::dataManager->settingsRepo->test_latency_url.toStdString();
    req.use_default_outbound = useDefault;
    req.max_concurrency = Configs::dataManager->settingsRepo->test_concurrent;
    req.test_timeout_ms = Configs::dataManager->settingsRepo->url_test_timeout_ms;
    req.xray_config = xrayConfig.toStdString();
    req.need_xray = !xrayConfig.isEmpty();
    for (const auto &xc : xrayFullConfigs) req.xray_full_configs.push_back(xc.toStdString());

    std::atomic_bool stopPolling{false};
    std::thread poller([=,this, &stopPolling]
    {
        bool ok;
        while (!stopPolling.load(std::memory_order_acquire))
        {
            QThread::msleep(200);
            if (stopPolling.load(std::memory_order_acquire)) break;
            auto resp = defaultClient->QueryURLTest(&ok);
            if (!ok || resp.results.empty())
            {
                continue;
            }

            bool needRefresh = false;
            QList<int> profileIDs;
            for (const auto& res : resp.results)
            {
                const QString tag = QString::fromStdString(res.outbound_tag.value());
                // Dual-tag mux probe: *-mux only feeds capability at final aggregate.
                if (tag.endsWith(QStringLiteral("-mux"))) continue;

                int entid = -1;
                if (!tag2entID.empty()) {
                    entid = tag2entID.count(tag) == 0 ? -1 : tag2entID[tag];
                }
                if (entid == -1) {
                    continue;
                }
                auto ent = Configs::dataManager->profilesRepo->GetProfile(entid);
                if (ent == nullptr) {
                    continue;
                }
                profileIDs << entid;

                // Capture result, defer write to UI thread to avoid race
                int latency_value;
                if (res.error.value().empty()) {
                    latency_value = res.latency_ms.value();
                } else {
                    if (QString::fromStdString(res.error.value()).contains("test aborted") ||
                        QString::fromStdString(res.error.value()).contains("context canceled")) {
                        latency_value = 0;
                    } else {
                        latency_value = -1;
                        MW_show_log(tr("[%1] test error: %2").arg(ent->outbound->DisplayTypeAndName(), QString::fromStdString(res.error.value())));
                    }
                }

                runOnUiThread([=, this] {
                    auto profile = Configs::dataManager->profilesRepo->GetProfile(entid);
                    if (profile) {
                        profile->SetLatency(latency_value);
                        Configs::dataManager->profilesRepo->Save(profile);
                    }
                }, true);

                needRefresh = true;
            }
            if (needRefresh)
            {
                {
                    QMutexLocker lock(&dataViewMutex_);
                    dataViewHtmlGenerator_.addTestProgress();
                }
                UpdateDataView(true);
                runOnUiThread([=,this]{
                    refresh_proxy_list(profileIDs);
                });
            }
        }
    });
    bool rpcOK;
    QString coreError;
    auto result = defaultClient->Test(&rpcOK, req, &coreError);
    stopPolling.store(true, std::memory_order_release);
    poller.join();
    //
    if (!rpcOK || result.results.empty()) {
        // A failed Test RPC (e.g. an Xray full config that needs geoip.dat /
        // geosite.dat) never yields per-result errors, so inspect the RPC error here
        // to offer the geo-asset download — the same flow profile start uses.
        if (!rpcOK) {
            QString ctxName = tr("a tested profile");
            if (entID != -1) {
                if (auto e = Configs::dataManager->profilesRepo->GetProfile(entID)) ctxName = e->outbound->DisplayTypeAndName();
            }
            handleXrayGeoAssetError(coreError, ctxName);
        }
        return;
    }

    // Per-ent: base (nomux) and optional mux probe outcomes for capability write.
    // 0=missing, 1=ok, -1=fail. Latency always from base only.
    QMap<int, int> baseOk; // entID -> 1/-1
    QMap<int, int> muxOk;

    for (const auto &res: result.results) {
        const QString tag = QString::fromStdString(res.outbound_tag.value());
        const bool isMuxProbe = tag.endsWith(QStringLiteral("-mux"));
        int thisEnt = entID;
        if (!tag2entID.empty()) {
            thisEnt = tag2entID.count(tag) == 0 ? -1 : tag2entID[tag];
        }
        if (thisEnt == -1) {
            MW_show_log(tr("Something is very wrong, the subject ent cannot be found!"));
            continue;
        }

        auto ent = Configs::dataManager->profilesRepo->GetProfile(thisEnt);
        if (ent == nullptr) {
            MW_show_log(tr("Profile manager data is corrupted, try again."));
            continue;
        }

        const bool ok = res.error.value().empty();
        const int outcome = ok ? 1 : -1;
        if (isMuxProbe) {
            muxOk[thisEnt] = outcome;
            continue; // never write latency from mux probe
        }
        baseOk[thisEnt] = outcome;

        int latency_value;
        if (ok) {
            latency_value = res.latency_ms.value();
        } else {
            if (QString::fromStdString(res.error.value()).contains("test aborted") ||
                QString::fromStdString(res.error.value()).contains("context canceled")) {
                latency_value = 0;
            } else {
                latency_value = -1;
                MW_show_log(tr("[%1] test error: %2").arg(ent->outbound->DisplayTypeAndName(), QString::fromStdString(res.error.value())));
            }
        }

        runOnUiThread([=, this] {
            auto profile = Configs::dataManager->profilesRepo->GetProfile(thisEnt);
            if (profile) {
                profile->SetLatency(latency_value);
                Configs::dataManager->profilesRepo->Save(profile);
            }
        }, true);
    }

    // Decision: ok|ok→yes, ok|fail→no, else keep unknown. Never touch explicit mux On/Off.
    for (auto it = baseOk.begin(); it != baseOk.end(); ++it) {
        const int id = it.key();
        if (!muxOk.contains(id)) continue;
        const int b = it.value();
        const int m = muxOk[id];
        int cap = 0;
        if (b == 1 && m == 1) cap = 1;
        else if (b == 1 && m == -1) cap = 2;
        else continue;
        runOnUiThread([=, this] {
            auto profile = Configs::dataManager->profilesRepo->GetProfile(id);
            if (!profile || profile->mux_capability != 0) return;
            profile->SetMuxCapability(cap);
            Configs::dataManager->profilesRepo->Save(profile);
            MW_show_log(tr("[%1] mux capability: %2")
                            .arg(profile->outbound ? profile->outbound->DisplayTypeAndName() : QString::number(id),
                                 cap == 1 ? QStringLiteral("yes") : QStringLiteral("no")));
        }, true);
    }
}

void MainWindow::runIPTest(const QString& config, const QString& xrayConfig, const QStringList& xrayFullConfigs, bool useDefault, const QStringList& outboundTags, const QMap<QString, int>& tag2entID, int entID) {
    if (stopSpeedtest.load()) {
        MW_show_log(tr("Profile test aborted"));
        return;
    }

    libcore::IPTestRequest req;
    for (const auto &item: outboundTags) {
        req.outbound_tags.push_back(item.toStdString());
    }
    req.config = config.toStdString();
    req.use_default_outbound = useDefault;
    req.max_concurrency = Configs::dataManager->settingsRepo->test_concurrent;
    req.test_timeout_ms = Configs::dataManager->settingsRepo->url_test_timeout_ms;
    req.xray_config = xrayConfig.toStdString();
    req.need_xray = !xrayConfig.isEmpty();
    for (const auto &xc : xrayFullConfigs) req.xray_full_configs.push_back(xc.toStdString());

    std::atomic_bool stopPolling{false};
    std::thread poller([=,this, &stopPolling]
    {
        bool ok;
        while (!stopPolling.load(std::memory_order_acquire))
        {
            QThread::msleep(200);
            if (stopPolling.load(std::memory_order_acquire)) break;
            auto resp = defaultClient->QueryIPTest(&ok);
            if (!ok || resp.results.empty())
            {
                continue;
            }

            bool needRefresh = false;
            QList<int> profileIDs;
            for (const auto& res : resp.results)
            {
                int entid = -1;
                if (!tag2entID.empty()) {
                    entid = tag2entID.count(QString::fromStdString(res.outbound_tag.value())) == 0 ? -1 : tag2entID[QString::fromStdString(res.outbound_tag.value())];
                }
                if (entid == -1) {
                    continue;
                }
                auto ent = Configs::dataManager->profilesRepo->GetProfile(entid);
                if (ent == nullptr) {
                    continue;
                }
                profileIDs << entid;

                // Capture result, defer write to UI thread to avoid race
                QString ip_out_value;
                QString test_country_value;
                if (res.error.value().empty()) {
                    ip_out_value = QString::fromStdString(res.ip.value());
                    test_country_value = InferCountryCode(QString::fromStdString(res.country_code.value()));
                } else {
                    if (!QString::fromStdString(res.error.value()).contains("test aborted") &&
                        !QString::fromStdString(res.error.value()).contains("context canceled")) {
                        MW_show_log(tr("[%1] IP test error: %2").arg(ent->outbound->DisplayTypeAndName(), QString::fromStdString(res.error.value())));
                    }
                    ip_out_value.clear();
                    test_country_value.clear();
                }

                runOnUiThread([=, this] {
                    auto profile = Configs::dataManager->profilesRepo->GetProfile(entid);
                    if (profile) {
                        profile->ip_out = ip_out_value;
                        profile->test_country = test_country_value;
                        Configs::dataManager->profilesRepo->Save(profile);
                    }
                }, true);

                needRefresh = true;
            }
            if (needRefresh)
            {
                {
                    QMutexLocker lock(&dataViewMutex_);
                    dataViewHtmlGenerator_.addTestProgress();
                }
                UpdateDataView(true);
                runOnUiThread([=,this]{
                    refresh_proxy_list(profileIDs);
                });
            }
        }
    });
    bool rpcOK;
    QString coreError;
    auto result = defaultClient->IPTest(&rpcOK, req, &coreError);
    stopPolling.store(true, std::memory_order_release);
    poller.join();
    //
    if (!rpcOK || result.results.empty()) {
        // Detect missing Xray geo assets from a failed IPTest RPC (see runURLTest).
        if (!rpcOK) {
            QString ctxName = tr("a tested profile");
            if (entID != -1) {
                if (auto e = Configs::dataManager->profilesRepo->GetProfile(entID)) ctxName = e->outbound->DisplayTypeAndName();
            }
            handleXrayGeoAssetError(coreError, ctxName);
        }
        return;
    }

    for (const auto &res: result.results) {
        if (!tag2entID.empty()) {
            entID = tag2entID.count(QString::fromStdString(res.outbound_tag.value())) == 0 ? -1 : tag2entID[QString::fromStdString(res.outbound_tag.value())];
        }
        if (entID == -1) {
            MW_show_log(tr("Something is very wrong, the subject ent cannot be found!"));
            continue;
        }

        auto ent = Configs::dataManager->profilesRepo->GetProfile(entID);
        if (ent == nullptr) {
            MW_show_log(tr("Profile manager data is corrupted, try again."));
            continue;
        }

        // Capture result, defer write to UI thread to avoid race
        QString ip_out_value;
        QString test_country_value;
        if (res.error.value().empty()) {
            ip_out_value = QString::fromStdString(res.ip.value());
            test_country_value = InferCountryCode(QString::fromStdString(res.country_code.value()));
        } else {
            if (!QString::fromStdString(res.error.value()).contains("test aborted") &&
                !QString::fromStdString(res.error.value()).contains("context canceled")) {
                MW_show_log(tr("[%1] IP test error: %2").arg(ent->outbound->DisplayTypeAndName(), QString::fromStdString(res.error.value())));
            }
            ip_out_value.clear();
            test_country_value.clear();
        }

        runOnUiThread([=, this] {
            auto profile = Configs::dataManager->profilesRepo->GetProfile(entID);
            if (profile) {
                profile->ip_out = ip_out_value;
                profile->test_country = test_country_value;
                Configs::dataManager->profilesRepo->Save(profile);
            }
        }, true);
    }
}

void MainWindow::urltest_current_group(const QList<int>& requestedIDs) {
    const auto profileIDs = withoutAutoSelectors(requestedIDs);
    if (profileIDs.isEmpty() || !acceptingOperations_.load()) return;
    if (!beginOrPromptBusyTest()) return;

    stopSpeedtest.store(false);
    operationCallPool->start([this, profileIDs]() {
        auto operationGuard = qScopeGuard([this] { speedtestOperation_.finish(Throne::OperationState::Running); });
        {
            QMutexLocker lock(&dataViewMutex_);
            dataViewHtmlGenerator_.seedLatencyTest(DataViewHtmlGenerator::LatencyTestPanelState::Kind::Url, profileIDs.size());
        }
        UpdateDataView(true);
        auto speedTestFunc = [=, this](const QList<std::shared_ptr<Configs::Profile>>& profileSlice, const QList<int>& ids) {
            auto buildObject = Configs::BuildTestConfig(profileSlice);
            if (!buildObject->error.isEmpty()) {
                MW_show_log(tr("Failed to build test config for batch: ") + buildObject->error);
                return;
            }

            // xray-full configs are folded into the single outboundTags test box
            // (their tags live in outboundTags), so they add no separate tests.
            const auto testCount = buildObject->fullConfigs.size() + (!buildObject->outboundTags.empty());
            std::latch completed(static_cast<std::ptrdiff_t>(testCount));
            for (const auto &entID: buildObject->fullConfigs.keys()) {
                auto configStr = buildObject->fullConfigs[entID];
                auto func = [this, &completed, configStr, entID]() {
                    auto countdown = qScopeGuard([&completed] { completed.count_down(); });
                    runURLTest(configStr, "", {}, true, {}, {}, entID);
                };
                parallelCoreCallPool->start(func);
            }

            if (!buildObject->outboundTags.empty()) {
                auto func = [this, buildObject, &completed]() {
                    auto countdown = qScopeGuard([&completed] { completed.count_down(); });
                    auto xrayConf = buildObject->isXrayNeeded ? QJsonObject2QString(buildObject->xrayConfig, false) : "";
                    runURLTest(QJsonObject2QString(buildObject->coreConfig, false),xrayConf, buildObject->xrayFullConfigs, false, buildObject->outboundTags, buildObject->tag2entID);
                };
                parallelCoreCallPool->start(func);
            }

            completed.wait();
            MW_show_log("URL test for batch done.");
            runOnUiThread([=,this]{
                refresh_proxy_list(ids);
            }, true);
        };
        std::shared_ptr<Configs::Group> currentGroup;
        for (int i=0;i<profileIDs.length();i+=100) {
            if (stopSpeedtest.load()) break;
            auto profileIDsSlice = profileIDs.mid(i, 100);
            auto profiles = Configs::dataManager->profilesRepo->GetProfileBatch(profileIDsSlice);
            if (!currentGroup && !profiles.isEmpty()) {
                currentGroup = Configs::dataManager->groupsRepo->GetGroup(profiles[0]->gid);
            }
            speedTestFunc(profiles, profileIDsSlice);
        }
        {
            QMutexLocker lock(&dataViewMutex_);
            dataViewHtmlGenerator_.clearTestSections();
        }
        UpdateDataView(true);
        if (currentGroup && currentGroup->auto_clear_unavailable) {
            MW_show_log("URL test finished, clearing unavailable profiles...");
            runOnUiThread([=, this] {
               clearUnavailableProfiles(false, profileIDs);
            }, true);
        }
        MW_show_log(tr("URL test finished!"));
    });
}

void MainWindow::stopTests() {
    stopSpeedtest.store(true);
    bool ok;
    defaultClient->StopTests(&ok);

    if (!ok) {
        MW_show_log(tr("Failed to stop tests"));
    }
}

bool MainWindow::beginOrPromptBusyTest() {
    if (speedtestOperation_.tryBegin(Throne::OperationState::Running)) return true;

    QMessageBox msg(
        QMessageBox::Warning,
        software_name,
        tr("A test is still running.\n\n"
           "Wait for it to finish, or stop it now (in-flight requests are cancelled; this is not pause/resume)."),
        QMessageBox::NoButton,
        this);
    auto *stopBtn = msg.addButton(tr("Stop testing"), QMessageBox::AcceptRole);
    auto *waitBtn = msg.addButton(tr("Wait"), QMessageBox::RejectRole);
    msg.setDefaultButton(waitBtn);
    msg.setEscapeButton(waitBtn);
    msg.exec();

    if (msg.clickedButton() == stopBtn) {
        stopTests();
        MW_show_log(tr("Stop testing requested; start again after the current test exits."));
    }
    return false;
}

void MainWindow::url_test_current() {
    if (!acceptingOperations_.load() || !beginOrPromptBusyTest()) return;

    last_test_time = QDateTime::currentSecsSinceEpoch();
    ui->label_running->setText(tr("Testing"));

    operationCallPool->start([=,this] {
        auto operationGuard = qScopeGuard([this] { speedtestOperation_.finish(Throne::OperationState::Running); });
        libcore::TestReq req;
        req.test_current = true;
        req.url = Configs::dataManager->settingsRepo->test_latency_url.toStdString();

        bool rpcOK;
        auto result = defaultClient->Test(&rpcOK, req);
        if (!rpcOK || result.results.empty()) {
            return;
        }

        auto latency = result.results[0].latency_ms.value();
        last_test_time = QDateTime::currentSecsSinceEpoch();

        runOnUiThread([=,this] {
            if (!result.results[0].error.value().empty()) {
                MW_show_log(QString("UrlTest error: %1").arg(QString::fromStdString(result.results[0].error.value())));
            }
            if (latency <= 0) {
                ui->label_running->setText(tr("Test Result") + ": " + tr("Unavailable"));
            } else if (latency > 0) {
                ui->label_running->setText(tr("Test Result") + ": " + QString("%1 ms").arg(latency));
            }
        }, true);
    });
}

void MainWindow::iptest_current_group(const QList<int>& requestedIDs) {
    const auto profileIDs = withoutAutoSelectors(requestedIDs);
    if (profileIDs.isEmpty() || !acceptingOperations_.load()) return;
    if (!beginOrPromptBusyTest()) return;

    stopSpeedtest.store(false);
    operationCallPool->start([this, profileIDs]() {
        auto operationGuard = qScopeGuard([this] { speedtestOperation_.finish(Throne::OperationState::Running); });
        {
            QMutexLocker lock(&dataViewMutex_);
            dataViewHtmlGenerator_.seedLatencyTest(DataViewHtmlGenerator::LatencyTestPanelState::Kind::Ip, profileIDs.size());
        }
        UpdateDataView(true);
        auto ipTestFunc = [=, this](const QList<std::shared_ptr<Configs::Profile>>& profileSlice, const QList<int>& ids) {
            auto buildObject = Configs::BuildTestConfig(profileSlice);
            if (!buildObject->error.isEmpty()) {
                MW_show_log(tr("Failed to build test config for batch: ") + buildObject->error);
                return;
            }

            // xray-full configs are folded into the single outboundTags test box
            // (their tags live in outboundTags), so they add no separate tests.
            const auto testCount = buildObject->fullConfigs.size() + (!buildObject->outboundTags.empty());
            std::latch completed(static_cast<std::ptrdiff_t>(testCount));
            for (const auto &entID: buildObject->fullConfigs.keys()) {
                auto configStr = buildObject->fullConfigs[entID];
                auto func = [this, &completed, configStr, entID]() {
                    auto countdown = qScopeGuard([&completed] { completed.count_down(); });
                    runIPTest(configStr, "", {}, true, {}, {}, entID);
                };
                parallelCoreCallPool->start(func);
            }

            if (!buildObject->outboundTags.empty()) {
                auto func = [this, buildObject, &completed]() {
                    auto countdown = qScopeGuard([&completed] { completed.count_down(); });
                    auto xrayConf = buildObject->isXrayNeeded ? QJsonObject2QString(buildObject->xrayConfig, false) : "";
                    runIPTest(QJsonObject2QString(buildObject->coreConfig, false), xrayConf, buildObject->xrayFullConfigs, false, buildObject->outboundTags, buildObject->tag2entID);
                };
                parallelCoreCallPool->start(func);
            }

            completed.wait();
            MW_show_log("IP test for batch done.");
            runOnUiThread([=,this]{
                refresh_proxy_list(ids);
            }, true);
        };
        for (int i = 0; i < profileIDs.length(); i += 100) {
            if (stopSpeedtest.load()) break;
            auto profileIDsSlice = profileIDs.mid(i, 100);
            auto profiles = Configs::dataManager->profilesRepo->GetProfileBatch(profileIDsSlice);
            ipTestFunc(profiles, profileIDsSlice);
        }
        {
            QMutexLocker lock(&dataViewMutex_);
            dataViewHtmlGenerator_.clearTestSections();
        }
        UpdateDataView(true);
        MW_show_log(tr("IP test finished!"));
    });
}

void MainWindow::speedtest_current_group(const QList<int>& requestedIDs, bool testCurrent)
{
    // testCurrent measures the live connection rather than a row, so it stays
    // valid for a running selector — it is the member actually carrying traffic.
    const auto profileIDs = testCurrent ? requestedIDs : withoutAutoSelectors(requestedIDs);
    if ((profileIDs.isEmpty() && !testCurrent) || !acceptingOperations_.load()) return;
    if (!beginOrPromptBusyTest()) return;

    currentUnderTest.store(testCurrent);

    stopSpeedtest.store(false);
    operationCallPool->start([this, profileIDs, testCurrent]() {
        // Fresh per-tag byte baselines for this speed-test session.
        { QMutexLocker lk(&speedtestCreditMu_); speedtestCredited_.clear(); }
        if (!testCurrent)
        {
            {
                QMutexLocker lock(&dataViewMutex_);
                dataViewHtmlGenerator_.seedSpeedTest(profileIDs.size());
            }
            UpdateDataView(true);
            auto speedTestFunc = [=, this](const QList<std::shared_ptr<Configs::Profile>>& profileSlice) {
                auto buildObject = Configs::BuildTestConfig(profileSlice);
                if (!buildObject->error.isEmpty()) {
                    MW_show_log(tr("Failed to build batch test config: ") + buildObject->error);
                    return;
                }

                for (const auto &entID: buildObject->fullConfigs.keys()) {
                    auto configStr = buildObject->fullConfigs[entID];
                    runSpeedTest(configStr, "", {}, true, false, {}, {}, entID);
                }

                if (!buildObject->outboundTags.empty()) {
                    auto xrayConf = buildObject->isXrayNeeded ? QJsonObject2QString(buildObject->xrayConfig, true) : "";
                    runSpeedTest(QJsonObject2QString(buildObject->coreConfig, false), xrayConf, buildObject->xrayFullConfigs, false, false, buildObject->outboundTags, buildObject->tag2entID, -1);
                }
            };
            int stepSize = Configs::dataManager->settingsRepo->speed_test_mode == Configs::TestConfig::COUNTRY ? 100 : 1;
            for (int i=0;i<profileIDs.length();i+=stepSize) {
                if (stopSpeedtest.load()) break;
                auto profileIDsSlice = profileIDs.mid(i, stepSize);
                auto profiles = Configs::dataManager->profilesRepo->GetProfileBatch(profileIDsSlice);
                speedTestFunc(profiles);
            }
        } else
        {
            {
                QMutexLocker lock(&dataViewMutex_);
                dataViewHtmlGenerator_.seedSpeedTest(1);
            }
            runSpeedTest("", "", {}, false, true, {}, {}, -1);
            currentUnderTest.store(false);
        }
        {
            QMutexLocker lock(&dataViewMutex_);
            dataViewHtmlGenerator_.clearTestSections();
        }
        UpdateDataView(true);
        runOnUiThread([=,this]{
            refresh_proxy_list(profileIDs);
            MW_show_log(tr("Speedtest finished!"));
        }, true);
        speedtestOperation_.finish(Throne::OperationState::Running);
    });
}

void MainWindow::creditSpeedtestTraffic(const std::shared_ptr<Configs::Profile>& profile, const QString& tag, qint64 curUp, qint64 curDown)
{
    if (profile == nullptr || tag.isEmpty()) return;
    if (Configs::dataManager->settingsRepo->disable_traffic_stats) return;
    QMutexLocker lk(&speedtestCreditMu_);
    auto& base = speedtestCredited_[tag];
    const qint64 dUp = curUp >= base.first ? curUp - base.first : curUp;
    const qint64 dDown = curDown >= base.second ? curDown - base.second : curDown;
    base = qMakePair(curUp, curDown);
    if (dUp <= 0 && dDown <= 0) return;

    Stats::trafficStatsManager->AddConfigDelta(profile->id, dUp, dDown);
    Stats::trafficStatsManager->AddAppDelta(Stats::SPEEDTEST_APP_NAME, "", dUp, dDown);

    profile->traffic_uplink.fetch_add(dUp, std::memory_order_relaxed);
    profile->traffic_downlink.fetch_add(dDown, std::memory_order_relaxed);
    Configs::dataManager->profilesRepo->SaveTraffic(profile);
}

void MainWindow::querySpeedtest(const QMap<QString, int>& tag2entID, bool testCurrent)
{
    bool ok;
    auto res = defaultClient->QueryCurrentSpeedTests(&ok);
    if (!ok || !res.is_running.value())
    {
        return;
    }
    auto profile = testCurrent ? std::atomic_load(&running) : Configs::dataManager->profilesRepo->GetProfile(tag2entID[QString::fromStdString(res.result.value().outbound_tag.value())]);
    if (profile == nullptr)
    {
        return;
    }
    creditSpeedtestTraffic(profile, QString::fromStdString(res.result.value().outbound_tag.value()),
                           res.result.value().ul_bytes.value(), res.result.value().dl_bytes.value());
    runOnUiThread([=, this]
    {
        {
            QMutexLocker lock(&dataViewMutex_);
            dataViewHtmlGenerator_.setSpeedtestProgress(profile->outbound->name, res.result.value());
        }
        UpdateDataView();

        if (res.result.value().error.value().empty() && !res.result.value().cancelled.value())
        {
            if (!res.result.value().dl_speed.value().empty()) profile->dl_speed = QString::fromStdString(res.result.value().dl_speed.value());
            if (!res.result.value().ul_speed.value().empty()) profile->ul_speed = QString::fromStdString(res.result.value().ul_speed.value());
            if (profile->latency <= 0 && res.result.value().latency.value() > 0) profile->SetLatency(res.result.value().latency.value());
            if (!res.result->server_country.value().empty()) profile->test_country = InferCountryCode(QString::fromStdString(res.result.value().server_country.value()));
            refresh_proxy_list({profile->id});
        }
    }, true);
}

void MainWindow::queryCountryTest(const QMap<QString, int>& tag2entID, bool testCurrent)
{
    bool ok;
    auto res = defaultClient->QueryCountryTestResults(&ok);
    if (!ok || res.results.empty())
    {
        return;
    }
    for (const auto& result : res.results)
    {
        {
            QMutexLocker lock(&dataViewMutex_);
            dataViewHtmlGenerator_.addTestProgress();
        }
        UpdateDataView();
        auto profile = testCurrent ? std::atomic_load(&running) : Configs::dataManager->profilesRepo->GetProfile(tag2entID[QString::fromStdString(result.outbound_tag.value())]);
        if (profile == nullptr)
        {
            return;
        }
        runOnUiThread([=, this]
        {
            if (result.error.value().empty() && !result.cancelled.value())
            {
                if (profile->latency <= 0 && result.latency.value() > 0) profile->SetLatency(result.latency.value());
                if (!result.server_country.value().empty()) profile->test_country = InferCountryCode(QString::fromStdString(result.server_country.value()));
                refresh_proxy_list({profile->id});
            }
        }, true);
    }
    UpdateDataView(true);
}


void MainWindow::runSpeedTest(const QString& config, const QString& xrayConfig, const QStringList& xrayFullConfigs, bool useDefault, bool testCurrent, const QStringList& outboundTags, const QMap<QString, int>& tag2entID, int entID)
{
    if (stopSpeedtest.load()) {
        MW_show_log(tr("Profile speed test aborted"));
        return;
    }

    libcore::SpeedTestRequest req;
    auto speedtestConf = Configs::dataManager->settingsRepo->speed_test_mode;
    for (const auto &item: outboundTags) {
        req.outbound_tags.push_back(item.toStdString());
    }
    req.config = config.toStdString();
    req.use_default_outbound = useDefault;
    req.test_download = speedtestConf == Configs::TestConfig::FULL || speedtestConf == Configs::TestConfig::DL;
    req.test_upload = speedtestConf == Configs::TestConfig::FULL || speedtestConf == Configs::TestConfig::UL;
    req.simple_download = speedtestConf == Configs::TestConfig::SIMPLEDL;
    req.simple_download_addr = Configs::dataManager->settingsRepo->simple_dl_url.toStdString();
    req.test_current = testCurrent;
    req.timeout_ms = Configs::dataManager->settingsRepo->speed_test_timeout_ms;
    req.only_country = speedtestConf == Configs::TestConfig::COUNTRY;
    req.country_concurrency = Configs::dataManager->settingsRepo->test_concurrent;
    req.xray_config = xrayConfig.toStdString();
    req.need_xray = !xrayConfig.isEmpty();
    for (const auto &xc : xrayFullConfigs) req.xray_full_configs.push_back(xc.toStdString());

    if (speedtestConf != Configs::TestConfig::COUNTRY) {
        {
            QMutexLocker lock(&dataViewMutex_);
            dataViewHtmlGenerator_.addTestProgress();
        }
        UpdateDataView();
    }

    // loop query result
    std::atomic_bool stopPolling{false};
    std::thread poller([=,this, &stopPolling]
    {
        while (!stopPolling.load(std::memory_order_acquire)) {
            QThread::msleep(100);
            if (stopPolling.load(std::memory_order_acquire)) break;
            if (speedtestConf == Configs::TestConfig::COUNTRY)
            {
                queryCountryTest(tag2entID, testCurrent);
            } else
            {
                querySpeedtest(tag2entID, testCurrent);
            }
        }
    });
    bool rpcOK;
    QString coreError;
    auto result = defaultClient->SpeedTest(&rpcOK, req, &coreError);
    stopPolling.store(true, std::memory_order_release);
    poller.join();
    //
    if (!rpcOK || result.results.empty()) {
        // Detect missing Xray geo assets from a failed SpeedTest RPC (see runURLTest).
        if (!rpcOK) {
            QString ctxName = tr("a tested profile");
            const auto currentRunning = std::atomic_load(&running);
            int nameId = testCurrent ? (currentRunning ? currentRunning->id : -1) : entID;
            if (nameId != -1) {
                if (auto e = Configs::dataManager->profilesRepo->GetProfile(nameId)) ctxName = e->outbound->DisplayTypeAndName();
            }
            handleXrayGeoAssetError(coreError, ctxName);
        }
        return;
    }

    for (const auto &res: result.results) {
        if (testCurrent) {
            const auto currentRunning = std::atomic_load(&running);
            entID = currentRunning ? currentRunning->id : -1;
        }
        else {
            entID = tag2entID.count(QString::fromStdString(res.outbound_tag.value())) == 0 ? -1 : tag2entID[QString::fromStdString(res.outbound_tag.value())];
        }
        if (entID == -1) {
            MW_show_log(tr("Something is very wrong, the subject ent cannot be found!"));
            continue;
        }

        auto ent = Configs::dataManager->profilesRepo->GetProfile(entID);
        if (ent == nullptr) {
            MW_show_log(tr("Profile manager data is corrupted, try again."));
            continue;
        }

        creditSpeedtestTraffic(ent, QString::fromStdString(res.outbound_tag.value()),
                               res.ul_bytes.value(), res.dl_bytes.value());

        if (res.cancelled.value()) continue;

        // Capture result on worker thread
        QString dl_speed_value;
        QString ul_speed_value;
        int latency_value = -1;
        QString test_country_value;
        QString error_msg;
        bool has_error = !res.error.value().empty();

        if (!has_error) {
            dl_speed_value = QString::fromStdString(res.dl_speed.value());
            ul_speed_value = QString::fromStdString(res.ul_speed.value());
            if (ent->latency <= 0 && res.latency.value() > 0) latency_value = res.latency.value();
            if (!res.server_country.value().empty()) test_country_value = InferCountryCode(QString::fromStdString(res.server_country.value()));
        } else {
            dl_speed_value = "N/A";
            ul_speed_value = "N/A";
            error_msg = tr("[%1] speed test error: %2").arg(ent->outbound->DisplayTypeAndName(), QString::fromStdString(res.error.value()));
        }

        // Defer write to UI thread to avoid race
        runOnUiThread([=, this] {
            auto profile = Configs::dataManager->profilesRepo->GetProfile(entID);
            if (!profile) return;

            profile->dl_speed = dl_speed_value;
            profile->ul_speed = ul_speed_value;
            if (has_error) {
                profile->SetLatency(-1);
                profile->test_country = "";
                MW_show_log(error_msg);
            } else {
                if (latency_value > 0) profile->SetLatency(latency_value);
                if (!test_country_value.isEmpty()) profile->test_country = test_country_value;
            }
            Configs::dataManager->profilesRepo->Save(profile);
        }, true);
    }
}

bool MainWindow::set_system_dns(bool set, bool save_set) {
    if (!Configs::dataManager->settingsRepo->enable_dns_server) {
        MW_show_log(tr("You need to enable hijack DNS server first"));
        return false;
    }
    if (!get_elevated_permissions(4)) {
        return false;
    }
    bool rpcOK;
    QString res;
    if (set) {
        res = defaultClient->SetSystemDNS(&rpcOK, false);
    } else {
        res = defaultClient->SetSystemDNS(&rpcOK, true);
    }
    if (!rpcOK) {
        MW_show_log(tr("Failed to set system dns: ") + res);
        return false;
    }
    if (save_set) Configs::dataManager->settingsRepo->system_dns_set = set;
    return true;
}

int MainWindow::get_profile_to_start() {
    auto ents = get_now_selected_list();
    if (ents.size() == 1) {
        return ents.first();
    }
    if (ents.isEmpty()) {
        if (last_running_profile_id >= 0 && Configs::dataManager->profilesRepo->GetProfile(last_running_profile_id) != nullptr) {
            return last_running_profile_id;
        }
        int rememberId = Configs::dataManager->settingsRepo->remember_id;
        if (rememberId >= 0 && Configs::dataManager->profilesRepo->GetProfile(rememberId) != nullptr) {
            return rememberId;
        }
        auto currentGroup = Configs::dataManager->groupsRepo->CurrentGroup();
        if (currentGroup) {
            auto profiles = currentGroup->Profiles();
            if (!profiles.isEmpty()) {
                int firstId = profiles.first();
                if (Configs::dataManager->profilesRepo->GetProfile(firstId) != nullptr) {
                    return firstId;
                }
            }
        }
    }
    return -1;
}

bool MainWindow::handleXrayGeoAssetError(const QString& error, const QString& contextName) {
    // The Xray config's routing referenced geoip:/geosite: rules. Two distinct
    // failures surface here (both when starting a profile via an in-band Start
    // error and when testing one via an RPC error payload). XRAY_LOCATION_ASSET
    // points at GetBasePath():
    //   1. The .dat asset isn't installed  -> "failed to open geoip.dat: ..."
    //   2. The .dat is installed but lacks the referenced category
    //                                       -> "failed to load code cn from geoip.dat: EOF"
    const bool refGeoip = error.contains("geoip.dat");
    const bool refGeosite = error.contains("geosite.dat");
    if (!refGeoip && !refGeosite) return false;

    runOnUiThread([=, this] {
        // A batch test can raise this for many profiles at once — only act once.
        if (m_xrayGeoAssetBusy) return;
        m_xrayGeoAssetBusy = true;
        // Small delay so any in-flight UI teardown (e.g. Connecting -> idle)
        // settles before the modal prompt appears.
        setTimeout([=, this] {
            const QString base = Configs::GetBasePath();
            const bool haveGeoip = QFile::exists(base + "/geoip.dat");
            const bool haveGeosite = QFile::exists(base + "/geosite.dat");

            const bool geoipLacksCategory = refGeoip && haveGeoip;
            const bool geositeLacksCategory = refGeosite && haveGeosite;
            if (geoipLacksCategory || geositeLacksCategory) {
                const QString whichFile = geositeLacksCategory ? "geosite.dat" : "geoip.dat";
                const QString ruleType = geositeLacksCategory ? "geosite" : "geoip";

                QString category;
                QRegularExpression re(QStringLiteral("code\\s+(\\S+)\\s+from"));
                const auto m = re.match(error);
                if (m.hasMatch()) category = m.captured(1);
                const QString needed = category.isEmpty()
                    ? tr("a required category")
                    : QStringLiteral("%1:%2").arg(ruleType, category);

                MessageBoxWarning(
                    tr("Geo asset missing category"),
                    tr("The Xray config \"%1\" needs \"%2\", but the installed %3 does "
                       "not contain it.\n\n"
                       "Re-downloading from the same source will not fix this — the data "
                       "file does not include that category. Set the GeoIP/GeoSite asset "
                       "URL in Settings to a source that provides \"%2\", then delete %3 "
                       "from the app folder and download it again.")
                        .arg(contextName, needed, whichFile));
                m_xrayGeoAssetBusy = false;
                return;
            }

            // Case 1: the referenced asset file is missing -> offer to download it.
            if (QMessageBox::question(this, tr("Geo asset files required"),
                    tr("The Xray config \"%1\" uses geoip/geosite routing rules, but the "
                       "required data files (geoip.dat / geosite.dat) are not installed.\n\n"
                       "Download them now?").arg(contextName)) != QMessageBox::Yes) {
                m_xrayGeoAssetBusy = false;
                return;
            }

            runOnNewThread([=, this] {
                QString dlErr;
                if (!haveGeoip) {
                    auto e = NetworkRequestHelper::DownloadAsset(Configs::dataManager->settingsRepo->xray_geoip_url, "geoip.dat");
                    if (!e.isEmpty()) dlErr += "geoip.dat: " + e + "\n";
                }
                if (!haveGeosite) {
                    auto e = NetworkRequestHelper::DownloadAsset(Configs::dataManager->settingsRepo->xray_geosite_url, "geosite.dat");
                    if (!e.isEmpty()) dlErr += "geosite.dat: " + e + "\n";
                }
                runOnUiThread([=, this] {
                    m_xrayGeoAssetBusy = false;
                    if (!dlErr.isEmpty()) {
                        MessageBoxWarning(tr("Geo asset download failed"), dlErr);
                    } else {
                        MW_show_log(tr("Downloaded Xray geo asset files."));
                        QMessageBox::information(this, tr("Geo assets installed"),
                            tr("Geo data files were downloaded successfully.\n\n"
                               "Please try again."));
                    }
                });
            });
        }, this, 300);
    });
    return true;
}

void MainWindow::profile_start(int _id) {
    if (Configs::dataManager->settingsRepo->prepare_exit) return;
#ifdef Q_OS_LINUX
    if (Configs::dataManager->settingsRepo->enable_dns_server && Configs::dataManager->settingsRepo->dns_server_listen_port <= 1024) {
        if (!get_elevated_permissions()) {
            MW_show_log(QString("Failed to get admin access, cannot listen on port %1 without it").arg(Configs::dataManager->settingsRepo->dns_server_listen_port));
            return;
        }
    }
#endif

    std::shared_ptr<Configs::Profile> ent = nullptr;
    if (_id >= 0) {
        ent = Configs::dataManager->profilesRepo->GetProfile(_id);
    } else {
        int startId = get_profile_to_start();
        if (startId >= 0) {
            ent = Configs::dataManager->profilesRepo->GetProfile(startId);
        }
    }
    if (ent == nullptr) return;

    last_running_profile_id = ent->id;

    if (select_mode) {
        emit profile_selected(ent->id);
        select_mode = false;
        refresh_status();
        return;
    }

    auto group = Configs::dataManager->groupsRepo->GetGroup(ent->gid);
    if (group == nullptr || group->archive) return;

    // Tun needs a privileged core. Checkbox elevates on toggle; cold start /
    // remember-tun / auto-start must elevate here too (setuid + core recycle).
    if (Configs::dataManager->settingsRepo->spmode_vpn && !Configs::IsAdmin()) {
        if (!get_elevated_permissions()) {
            MW_show_log(tr("Failed to get admin access; cannot start with Tun enabled"));
            MessageBoxWarning(
                tr("Tun mode unavailable"),
                tr("Tun requires Core root privileges. Grant privileges and try again, "
                   "or uncheck Tun and start without it."));
            return;
        }
        // Elevation may recycle the unprivileged core. Queue resume on CoreStarted
        // (recycle only auto-queues when started_id was already set).
        if (!Configs::dataManager->settingsRepo->core_running) {
            QMutexLocker lock(&coreProcessMutex);
            core_process->start_profile_when_core_is_up = ent->id;
            return;
        }
    }

    // An auto selector with more candidates than it can run has to know which
    // ones are best before the config is built, otherwise the choice is
    // arbitrary. Measuring hundreds of profiles blocks, so hop off the UI
    // thread and come back to start once the ranking is in.
    if (ent->type == "autoselector" && !auto_selector_ranked) {
        const auto plan = Configs::PlanAutoSelector(ent);
        if (plan.error.isEmpty() && plan.needsRanking) {
            auto_selector_ranked = true;
            const int startId = ent->id;
            runOnNewThread([=, this] {
                rank_auto_selector(ent);
                runOnUiThread([=, this] {
                    auto_selector_ranked = false;
                    profile_start(startId);
                });
            });
            return;
        }
    }
    auto_selector_ranked = false;

    // Build config on the caller thread (usually UI). Follow-mode WARP must not
    // block here: password prompt + readiness polling can take tens of seconds.
    // AdBlock uses a local .srs only; if missing we still build (without AdBlock)
    // and try a pre-download in the worker before Start, then rebuild via holder.
    // Underlay device comes from the 2s poll cache — never fork status on UI.
    auto resultHolder = std::make_shared<std::shared_ptr<Configs::BuildConfigResult>>(
        Configs::BuildSingBoxConfig(ent, false, cachedWarpUnderlayForConfig()));
    if (!(*resultHolder)->error.isEmpty()) {
        MessageBoxWarning(tr("BuildConfig return error"), (*resultHolder)->error);
        return;
    }

    auto profile_start_stage2 = [=, this] {
        const auto &result = *resultHolder;
        // Read back from the config actually handed to the core, so this cannot
        // drift from what the core is really bound to.
        warpInterfaceInRunningConfig =
            result->coreConfig["route"].toObject()["default_interface"].toString();
        libcore::LoadConfigReq req;
        req.profile_id = ent->id;
        req.core_config = QJsonObject2QString(result->coreConfig, true).toStdString();
        req.tun_ipv4_cidr = result->tunIPv4CIDR.toStdString();
        req.disable_stats = Configs::dataManager->settingsRepo->disable_traffic_stats;
        req.xray_config = QJsonObject2QString(result->xrayConfig, true).toStdString();
        req.need_xray = !result->xrayConfig.isEmpty();
        if (req.need_xray) {
            // Outbound server-domain resolution for the live Xray instance is
            // wired in the core (ThroneWiring), not baked into the config: point
            // it at sing-box's loopback DNS-in with the user's direct-DNS
            // strategy and let the instance build the resolver internally. Test
            // instances build their own req and leave these empty.
            req.xray_outbound_dns_address = ("127.0.0.1:" + QString::number(Configs::dataManager->settingsRepo->core_dns_in_port)).toStdString();
            req.xray_outbound_dns_strategy = Configs::getXrayOutboundDomainStrategy().toStdString();
            // A pool's Xray members may be nothing but bench-tier candidates the
            // selector probes once a cycle, so let the core keep the sidecar
            // cold between dials. The idle window has to outlast the probe
            // interval, otherwise a member the selector is actively watching
            // would start and stop the instance on every round.
            if (auto selector = ent->AutoSelector(); selector != nullptr) {
                req.xray_lazy_start = true;
                req.xray_idle_seconds = std::max(120, selector->intervalSec * 2);
            }
        }
        if (result->extraCoreData && !result->extraCoreData->path.isEmpty())
        {
            req.need_extra_process = true;
            req.extra_process_path = result->extraCoreData->path.toStdString();
            req.extra_process_args = result->extraCoreData->args.toStdString();
            req.extra_process_conf = result->extraCoreData->config.toStdString();
            req.extra_no_out = result->extraCoreData->noLog;
        }
        //
        bool rpcOK;
        QString error = defaultClient->Start(&rpcOK, req,
            [this] { return m_profileStartCancelRequested.load() || !acceptingOperations_.load(); });
        if (!rpcOK) {
            return false;
        }
        if (!error.isEmpty()) {
            // The Xray config's routing referenced geoip:/geosite: tags but the .dat
            // asset(s) aren't installed. Handle this out-of-band: fail this start
            // attempt right away — blocking here to download would trip the "no
            // response" restart prompt — while handleXrayGeoAssetError asynchronously
            // prompts and downloads. We deliberately don't auto-start; the env var is
            // already set on the live core, so starting the profile again picks the
            // assets up with no core restart.
            if (handleXrayGeoAssetError(error, ent->outbound->DisplayTypeAndName())) {
                return false;
            }
            if (error.contains("Fwpm", Qt::CaseInsensitive)) {
                runOnUiThread([=, this] {
                    MessageBoxWarning(
                        tr("Strict routing unavailable"),
                        tr("Windows could not enable strict routing. Open Tun Settings, "
                           "disable Strict Route, and start the profile again.\n\n"
                           "Disabling Strict Route may cause DNS leaks.\n\nError: %1").arg(error));
                });
                return false;
            }
            if (error.contains("configure tun interface")) {
                const int profileId = ent->id;
                runOnUiThread([=, this] {
                    const bool privileged = Configs::IsAdmin(true);
                    QMessageBox msg(
                        QMessageBox::Warning,
                        tr("Tun start failed"),
                        privileged
                            ? tr("Core could not configure the Tun interface.\n\n"
                                 "You can start without Tun, reset Core, or cancel.\n\n"
                                 "Error: %1").arg(error)
                            : tr("Core is not privileged, so Tun cannot be configured "
                                 "(operation not permitted).\n\n"
                                 "Grant Core privileges and retry, start without Tun, "
                                 "or reset Core.\n\n"
                                 "Error: %1").arg(error),
                        QMessageBox::NoButton,
                        this);
                    QPushButton *grantBtn = nullptr;
                    if (!privileged) {
                        grantBtn = msg.addButton(tr("Grant privileges and retry"), QMessageBox::ActionRole);
                    }
                    auto *withoutTunBtn = msg.addButton(tr("Start without Tun"), QMessageBox::ActionRole);
                    auto *resetBtn = msg.addButton(tr("Reset Core"), QMessageBox::ActionRole);
                    auto *cancelBtn = msg.addButton(tr("Cancel"), QMessageBox::ActionRole);
                    msg.setDefaultButton(grantBtn ? grantBtn : withoutTunBtn);
                    msg.setEscapeButton(cancelBtn);
                    msg.exec();

                    auto *clicked = msg.clickedButton();
                    if (clicked == grantBtn) {
                        if (!get_elevated_permissions()) {
                            MW_show_log(tr("Failed to get admin access; Tun start aborted"));
                            return;
                        }
                        if (!Configs::dataManager->settingsRepo->core_running) {
                            QMutexLocker lock(&coreProcessMutex);
                            core_process->start_profile_when_core_is_up = profileId;
                            return;
                        }
                        profile_start(profileId);
                    } else if (clicked == withoutTunBtn) {
                        auto &settings = Configs::dataManager->settingsRepo;
                        if (settings->spmode_vpn) {
                            settings->spmode_vpn = false;
                            settings->Save();
                            refresh_status();
                        }
                        profile_start(profileId);
                    } else if (clicked == resetBtn) {
                        StopVPNProcess();
                    }
                }, true);
                return false;
            }
            runOnUiThread([=] { MessageBoxWarning(tr("LoadConfig return error"), error); }, true);
            return false;
        }
        //
        Stats::trafficLooper->SetChainGroups(result->chainGroups);
        Stats::trafficLooper->loop_enabled = true;
        Stats::connection_lister->suspend = false;
        Stats::autoSelectorMonitor->SetBuild(result->autoSelectors);
        if (!result->autoSelectors.isEmpty()) {
            const auto& info = result->autoSelectors.first();
            if (auto selector = ent->AutoSelector(); selector != nullptr) {
                QList<int> builtIDs;
                QHash<int, QString> names;
                for (const auto& [tag, member] : info.members) {
                    if (member == nullptr) continue;
                    builtIDs << member->id;
                    names.insert(member->id, member->outbound ? member->outbound->DisplayName() : member->name);
                }
                const auto now = QDateTime::currentSecsSinceEpoch();
                selector->lastBuilt = builtIDs;
                selector->lastBuiltAt = now;
                selector->RecordHistory(builtIDs, names, now);
                Configs::dataManager->profilesRepo->Save(ent);
                MW_show_log(tr("[Auto selector] Running the best %1 of %2 ranked profiles.")
                                .arg(builtIDs.size())
                                .arg(selector->pool.size()));
            }
        }

        auto &settings = Configs::dataManager->settingsRepo;
        settings->session_system_proxy = settings->spmode_system_proxy;
        settings->session_vpn = settings->spmode_vpn;
        settings->UpdateStartedId(ent->id);
        if (settings->spmode_system_proxy && !set_system_proxy(true)) {
            settings->spmode_system_proxy = false;
            settings->session_system_proxy = false;
            MW_show_log(tr("[System Proxy] failed to enable system proxy"));
        }

        runOnUiThread([=, this] {
            std::atomic_store(&running, ent);
            refresh_status();
            refresh_proxy_list({ent->id});
            // Reveals the Tools entry and seeds the data-view panel before the
            // first poll lands, so a selector never starts up invisibly.
            refresh_auto_selector_view();

            auto resp = NetworkRequestHelper::HttpGet("http://ip-api.com/json/", false, true);
            if (resp.error.isEmpty()) {
                QJsonDocument doc = QJsonDocument::fromJson(resp.data);
                if (doc.isObject()) {
                    QJsonObject obj = doc.object();
                    QString city = obj["city"].toString();
                    QString countryName = obj["country"].toString();
                    QString countryCode = obj["countryCode"].toString();
                    if (const auto r = std::atomic_load(&running)) r->runningCountryInfo = QString("%1 %2, %3").arg(CountryCodeToFlag(countryCode), countryName, city);
                    refresh_status();
                }
            }
        }, true);

        return true;
    };

    if (!acceptingOperations_.load()) return;
    if (!profileOperation_.tryBegin(Throne::OperationState::Starting)) {
        const auto message = profileOperation_.state() == Throne::OperationState::Stopping
            ? tr("Another profile is stopping...")
            : tr("Another profile is starting...");
        MessageBoxWarning(software_name, message);
        return;
    }

    // check core state
    if (!Configs::dataManager->settingsRepo->core_running) {
        // Re-enter profile_start after core is up so WARP preflight still runs then.
        // Wait for Start so start_profile_when_core_is_up is set before any
        // reconnect race can clear it.
        runOnThread(
            [=, this] {
                MW_show_log(tr("Try to start the config, but the core has not listened to the RPC port, so start it..."));
                {
                    QMutexLocker lock(&coreProcessMutex);
                    core_process->start_profile_when_core_is_up = ent->id;
                }
                // startCoreDetached arms pre-IPC death watch; bare Start would hang if core dies first.
                if (!startCoreDetached()) {
                    MW_show_log("[Error] Failed to start Core process (missing ThroneCore beside the app binary?)");
                    QMutexLocker lock(&coreProcessMutex);
                    core_process->start_profile_when_core_is_up = -1;
                }
            },
            DS_cores, true);
        profileOperation_.finish(Throne::OperationState::Starting);
        return; // CoreStarted handler re-enters profile_start when RPC is ready
    }

    // timeout message
    auto restartMsgbox = new QMessageBox(QMessageBox::Question, software_name, tr("If there is no response for a long time, it is recommended to restart the software."),
                                         QMessageBox::Yes | QMessageBox::No, this);
    // buttonClicked, not accepted: QMessageBox closes with the button's role, not
    // QDialog::Accepted, so an accepted() connection never fires for Yes.
    connect(restartMsgbox, &QMessageBox::buttonClicked, this, [=, this](QAbstractButton *b) {
        if (restartMsgbox->standardButton(b) == QMessageBox::Yes) {
            MW_dialog_message(MwMessage::RestartProgram, {});
        }
    });
    auto restartMsgboxTimer = new MessageBoxTimer(this, restartMsgbox, kStartRestartHintTimeoutMs);

    // Show the "Connecting" state until the start resolves below.
    m_profileStartCancelRequested.store(false);
    runOnUiThread([this] {
        m_profileConnecting = true;
        refresh_startstop_button();
    });

    operationCallPool->start([=, this] {
        auto finishStartUi = [=, this](bool canceled) {
            runOnUiThread([=, this] {
                restartMsgboxTimer->cancel();
                restartMsgboxTimer->deleteLater();
                restartMsgbox->deleteLater();
                m_profileConnecting = false;
                m_profileStartCancelRequested.store(false);
                refresh_startstop_button();
                refreshWarpRuntimeStatus();
                refreshWarpButton();
                icon_status = -1;
                refresh_status();
                if (canceled) {
                    MW_show_log(tr("Profile start canceled"));
                }
            }, true);
            profileOperation_.finish(Throne::OperationState::Starting);
        };
        auto startCanceled = [this] {
            return m_profileStartCancelRequested.load() || !acceptingOperations_.load();
        };

        // If WARP is preferred/on, preflight the system tunnel before proxy start.
        // Admin password + readiness poll stay off the UI thread.
        // Toolbar owns tunnel lifecycle; do not roll WARP back if proxy start fails.
        bool heldWarpOp = false;
        if (Configs::dataManager->settingsRepo->enable_warp) {
            if (startCanceled()) {
                finishStartUi(true);
                return;
            }
            // Wait for WARP lock in small steps so Cancel can land.
            for (int waited = 0; waited <= 60000; waited += 50) {
                if (startCanceled()) break;
                if (acquireWarpOp(0)) {
                    heldWarpOp = true;
                    break;
                }
                std::this_thread::sleep_for(std::chrono::milliseconds(50));
            }
            if (startCanceled()) {
                if (heldWarpOp) releaseWarpOp();
                finishStartUi(true);
                return;
            }
            if (!heldWarpOp) {
                // Do not start proxy while another WARP op holds the tunnel lock;
                // that would race Up/Down and leave status/preference desynced.
                if (startCanceled() || !askContinueWithoutWarp(tr("WARP is busy; could not acquire tunnel lock"))) {
                    finishStartUi(startCanceled());
                    return;
                }
            } else {
                const auto warpGeneration = warpStatusGeneration.load();
                QString warpError;
                const bool ready = ensureWarpReady(nullptr, &warpError, startCanceled);
                if (startCanceled()) {
                    releaseWarpOp();
                    finishStartUi(true);
                    return;
                }
                const auto info = Configs_sys::WarpProcess::RuntimeInfo();
                runOnUiThread([this, warpGeneration, info] {
                    if (!acceptingOperations_.load()
                        || warpGeneration != warpStatusGeneration.load()) {
                        releaseWarpOp();
                        return;
                    }
                    warpRuntimeStatus = info.status;
                    warpTransport = info.transport;
                    warpInterfaceName = info.interfaceName;
                    warpDesiredOn = Configs::dataManager->settingsRepo->enable_warp;
                    releaseWarpOp();
                    refreshWarpButton();
                }, true);
                heldWarpOp = false;
                if (startCanceled()) {
                    finishStartUi(true);
                    return;
                }
                if (!ready && !askContinueWithoutWarp(warpError)) {
                    finishStartUi(startCanceled());
                    return;
                }
                // ensureWarpReady may have just brought the underlay up; rebuild with
                // the fresh device name so the core is not started without the pin.
                const auto underlay = Configs_sys::WarpUnderlayInterfaceForConfig(
                    Configs::dataManager->settingsRepo->spmode_vpn,
                    Configs::dataManager->settingsRepo->enable_warp,
                    info.status,
                    info.interfaceName);
                if (!underlay.isEmpty()
                    && (*resultHolder)->coreConfig["route"].toObject()["default_interface"].toString()
                           != underlay) {
                    *resultHolder = Configs::BuildSingBoxConfig(ent, false, underlay);
                    if (!(*resultHolder)->error.isEmpty()) {
                        runOnUiThread([err = (*resultHolder)->error] {
                            MessageBoxWarning(tr("BuildConfig return error"), err);
                        }, true);
                        finishStartUi(false);
                        return;
                    }
                }
            }
        }

        if (startCanceled()) {
            if (heldWarpOp) releaseWarpOp();
            finishStartUi(true);
            return;
        }

        // AdBlock: never remote-fetch during core initialize. Ensure local asset
        // before Start; if download requires a live node but none is up, tell the user.
        if (Configs::dataManager->settingsRepo->adblock_enable && !Configs::adblockRulesetAvailable()) {
            const auto &settings = Configs::dataManager->settingsRepo;
            const bool wantsProxy = settings->net_use_proxy || settings->spmode_system_proxy;
            if (wantsProxy && settings->started_id < 0) {
                runOnUiThread([this] {
                    MessageBoxWarning(
                        tr("AdBlock ruleset"),
                        tr("AdBlock is enabled and network downloads are set to use the proxy, "
                           "but no profile is running yet.\n\n"
                           "Start once without AdBlock (or disable \"Use proxy for network requests\"), "
                           "then re-enable AdBlock so the ruleset can download through the node.\n\n"
                           "Profile will start without AdBlock this time."));
                }, true);
                MW_show_log(tr("[AdBlock] Skipped: ruleset download requires a running node (proxy download enabled)."));
            } else {
                MW_show_log(tr("[AdBlock] Downloading ruleset…"));
                const QString err = NetworkRequestHelper::DownloadAsset(
                    Configs::adblockRulesetUrl(), Configs::adblockRulesetFileName());
                if (!err.isEmpty()) {
                    const QString detail = wantsProxy
                        ? tr("AdBlock ruleset update is configured to go through the proxy node, "
                             "but the download failed.\n\n"
                             "Check that the selected node is reachable, or temporarily disable "
                             "\"Use proxy for network requests\" and retry.\n\n"
                             "Error: %1\n\nProfile will start without AdBlock this time.").arg(err)
                        : tr("Could not download the AdBlock ruleset (direct download failed).\n\n"
                             "Check network / ruleset mirror settings, then re-enable AdBlock.\n\n"
                             "Error: %1\n\nProfile will start without AdBlock this time.").arg(err);
                    runOnUiThread([this, detail] {
                        MessageBoxWarning(tr("AdBlock ruleset"), detail);
                    }, true);
                    MW_show_log(tr("[AdBlock] Download failed: %1").arg(err));
                } else {
                    MW_show_log(tr("[AdBlock] Ruleset saved to %1").arg(Configs::adblockRulesetPath()));
                    // Rebuild so Start loads local rule-set + reject rule.
                    // Keep whatever underlay pin the earlier build already chose.
                    const auto underlay =
                        (*resultHolder)->coreConfig["route"].toObject()["default_interface"].toString();
                    *resultHolder = Configs::BuildSingBoxConfig(ent, false, underlay);
                    if (!(*resultHolder)->error.isEmpty()) {
                        runOnUiThread([err = (*resultHolder)->error] {
                            MessageBoxWarning(tr("BuildConfig return error"), err);
                        }, true);
                        if (heldWarpOp) releaseWarpOp();
                        finishStartUi(false);
                        return;
                    }
                }
            }
        }

        if (startCanceled()) {
            if (heldWarpOp) releaseWarpOp();
            finishStartUi(true);
            return;
        }

        // Profile switch leaves WARP alone; only the toolbar owns tunnel lifecycle.
        const auto currentRunning = std::atomic_load(&running);
        const bool canStart = currentRunning == nullptr || profile_stop_impl(false, false, currentRunning->id);
        if (startCanceled()) {
            if (heldWarpOp) releaseWarpOp();
            // If we stopped the previous profile for a switch, leave it stopped.
            finishStartUi(true);
            return;
        }
        if (canStart) {
            MW_show_log(">>>>>>>> " + tr("Starting profile %1").arg(ent->outbound->DisplayTypeAndName()));
            if (!profile_start_stage2()) {
                MW_show_log("<<<<<<<< " + tr("Failed to start profile %1").arg(ent->outbound->DisplayTypeAndName()));
            } else if (startCanceled()) {
                // Start completed after cancel: tear it back down immediately.
                MW_show_log(tr("Profile started after cancel request; stopping…"));
                profile_stop_impl(false, true, ent->id);
            }
        }
        if (heldWarpOp) releaseWarpOp();
        finishStartUi(startCanceled());
    });
}

bool MainWindow::profile_stop_impl(bool crash, bool manual, int id) {
    auto profile_stop_stage2 = [=,this] {
        if (currentUnderTest.load()) {
            bool ok;
            defaultClient->StopTests(&ok);
            if (!ok) MW_show_log("Failed to stop profile tests!");
        }
        if (!crash) {
            bool rpcOK;
            QString error = defaultClient->Stop(&rpcOK);
            if (rpcOK && !error.isEmpty()) {
                runOnUiThread([=,this] { MessageBoxWarning(tr("Stop return error"), error); }, true);
                return false;
            } else if (!rpcOK) {
                runOnUiThread([=, this] {
                    MessageBoxWarning(tr("Stop failed"), tr("Failed to stop, please restart the program."));
                }, true);
                return false;
            }
        }
        if (Configs::dataManager->settingsRepo->spmode_system_proxy) set_system_proxy(false);
        return true;
    };

    runOnUiThread([this] {
        UpdateConnectionListWithRecreate({});
        m_profileDisconnecting = true;
        refresh_startstop_button();
    }, true);

    Stats::autoSelectorMonitor->Clear();
    runOnUiThread([this] { refresh_auto_selector_view(); });

    Stats::trafficLooper->loop_enabled = false;
    Stats::connection_lister->suspend = true;
    // UpdateAll takes loop_mutex itself around apply; do not hold it here.
    Stats::trafficLooper->UpdateAll();
    // Flush the final per-profile totals (only persisted every few seconds
    // during the session) and the partial minute bucket before going down.
    Stats::trafficLooper->PersistTraffic();
    Stats::trafficStatsManager->Flush();

    QMessageBox* restartMsgbox = nullptr;
    MessageBoxTimer* restartMsgboxTimer = nullptr;
    runOnUiThread([=, this, &restartMsgbox, &restartMsgboxTimer] {
        restartMsgbox = new QMessageBox(QMessageBox::Question, software_name, tr("If there is no response for a long time, it is recommended to restart the software."),
                         QMessageBox::Yes | QMessageBox::No, this);
        auto *box = restartMsgbox;
        connect(box, &QMessageBox::buttonClicked, this, [=, this](QAbstractButton *b) {
            if (box->standardButton(b) == QMessageBox::Yes) {
                MW_dialog_message(MwMessage::RestartProgram, {});
            }
        });
        restartMsgboxTimer = new MessageBoxTimer(this, restartMsgbox, kStopRestartHintTimeoutMs);
    }, true);

    auto stoppingProfile = std::atomic_load(&running);
    if (stoppingProfile) {
        MW_show_log(">>>>>>>> " + tr("Stopping profile %1").arg(stoppingProfile->outbound->DisplayTypeAndName()));
    }
    const bool stopped = profile_stop_stage2();
    if (!stopped) {
        MW_show_log("<<<<<<<< " + tr("Failed to stop, please restart the program."));
    }

    if (stopped && manual) Configs::dataManager->settingsRepo->UpdateStartedId(-1919);
    // Unified WARP is independent of proxy stop; only the toolbar tears the tunnel down.

    runOnUiThread([=, this] {
        if (stopped) {
            std::atomic_store(&running, std::shared_ptr<Configs::Profile>{});
        } else {
            Stats::trafficLooper->loop_enabled = true;
            Stats::connection_lister->suspend = false;
        }
        restartMsgboxTimer->cancel();
        restartMsgboxTimer->deleteLater();
        restartMsgbox->deleteLater();
        m_profileDisconnecting = false;
        refresh_status();
        refresh_proxy_list({id});
        refreshWarpRuntimeStatus();
        refreshWarpButton();
    }, true);
    return stopped;
}

bool MainWindow::profile_stop(bool crash, bool block, bool manual) {
    return profile_stop(crash, block, manual, ProfileStopAdmission::Normal);
}

bool MainWindow::profile_stop(bool crash, bool block, bool manual, ProfileStopAdmission admission) {
    const auto currentRunning = std::atomic_load(&running);
    if (currentRunning == nullptr) {
        return true;
    }
    if (admission == ProfileStopAdmission::Normal && !acceptingOperations_.load()) return false;
    if (!profileOperation_.tryBegin(Throne::OperationState::Stopping)) return false;
    const auto id = currentRunning->id;
    auto task = [=, this] {
        const bool stopped = profile_stop_impl(crash, manual, id);
        profileOperation_.finish(Throne::OperationState::Stopping);
        return stopped;
    };

    if (!block) {
        operationCallPool->start([task] { task(); });
        return true;
    }

    std::atomic_bool stopped{false};
    QEventLoop loop;
    operationCallPool->start([task, &loop, &stopped] {
        stopped.store(task());
        QMetaObject::invokeMethod(&loop, "quit", Qt::QueuedConnection);
    });
    loop.exec();
    return stopped.load();
}
