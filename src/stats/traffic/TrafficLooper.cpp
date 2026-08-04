#include "include/stats/traffic/TrafficLooper.hpp"

#include "include/api/RPC.h"
#include "include/ui/mainwindow_interface.h"

#include <QJsonDocument>
#include <QPointer>
#include <QSet>

#include "include/database/ProfilesRepo.h"
#include "include/database/GroupsRepo.h"
#include "include/database/DatabaseManager.h"
#include "include/global/InterruptibleWait.hpp"
#include "include/stats/traffic/TrafficStatsManager.hpp"


namespace Stats {

    TrafficLooper *trafficLooper = new TrafficLooper;

    namespace {
        constexpr int kTrafficSaveIntervalSecs = 10;
    }

    void TrafficLooper::UpdateAll() {
        if (Configs::dataManager->settingsRepo->disable_traffic_stats) {
            return;
        }

        // RPC outside loop_mutex: QueryStats can block on core IPC.
        auto resp = API::defaultClient->QueryStats();

        QMutexLocker lock(&loop_mutex);
        if (!proxy || !direct) return;
        const auto now = elapsed_timer.elapsed();

        proxy->uplink_rate = 0;
        proxy->downlink_rate = 0;

        // For each chain group, read the matched-outbound's delta-since-last-query
        // and credit it to every user-visible profile in the chain. Aggregate
        // rates from all groups into the proxy entry for the status bar.
        for (auto& group : groups) {
            const auto tagKey = group.watchTag.toStdString();
            if (!resp.ups.contains(tagKey)) continue;
            const auto interval = now - group.last_update;
            group.last_update = now;
            if (interval <= 0) continue;
            const auto up = resp.ups.at(tagKey);
            const auto down = resp.downs.at(tagKey);
            // An auto-selector contributes one group per pool member, all but
            // one of them idle at any moment. Skipping the zero deltas keeps a
            // 300-member pool from doing 300 no-op stat writes every second.
            if (up != 0 || down != 0) {
                for (auto& profile : group.profiles) {
                    if (!profile) continue;
                    profile->traffic_uplink.fetch_add(up, std::memory_order_relaxed);
                    profile->traffic_downlink.fetch_add(down, std::memory_order_relaxed);
                    // Mirror the per-profile crediting into the time-series module.
                    trafficStatsManager->AddConfigDelta(profile->id, up, down);
                }
                group.dirty = true;
            }
            group.uplink_rate = static_cast<double>(up) * 1000.0 / static_cast<double>(interval);
            group.downlink_rate = static_cast<double>(down) * 1000.0 / static_cast<double>(interval);
            proxy->uplink_rate += group.uplink_rate;
            proxy->downlink_rate += group.downlink_rate;
        }

        // direct: not part of any chain group, tracked on its own for the
        // status-bar split.
        direct->uplink_rate = 0;
        direct->downlink_rate = 0;
        const std::string directTag = "direct";
        if (resp.ups.contains(directTag)) {
            const auto interval = now - direct_last_update;
            direct_last_update = now;
            if (interval > 0) {
                const auto up = resp.ups.at(directTag);
                const auto down = resp.downs.at(directTag);
                direct->uplink_rate = static_cast<double>(up) * 1000.0 / static_cast<double>(interval);
                direct->downlink_rate = static_cast<double>(down) * 1000.0 / static_cast<double>(interval);
                trafficStatsManager->AddConfigDelta(DIRECT_STAT_PROFILE_ID, up, down);
            }
        }
    }

    void TrafficLooper::Start() {
        worker.Start([this](std::stop_token stopToken) { Loop(stopToken); });
    }

    void TrafficLooper::Stop() {
        loop_enabled.store(false, std::memory_order_release);
        worker.Stop([this] {
            QMutexLocker lock(&wait_mutex);
            wait_condition.wakeAll();
        });
        looping.store(false, std::memory_order_release);
    }

    void TrafficLooper::Loop(std::stop_token stopToken) {
        {
            QMutexLocker lock(&loop_mutex);
            if (!elapsed_timer.isValid()) elapsed_timer.start();
        }
        int secs_since_save = 0;
        while (!stopToken.stop_requested()) {
            if (!Throne::waitForStopOrTimeout(wait_condition, wait_mutex, stopToken, 1000)) break;

            if (Configs::dataManager->settingsRepo->disable_traffic_stats) {
                continue;
            }

            if (!loop_enabled.load(std::memory_order_acquire)) {
                const bool wasLooping = looping.exchange(false, std::memory_order_acq_rel);
                publishRateSnapshotUnlocked({});
                const QPointer<MainWindow> window(GetMainWindow());
                runOnUiThread([window, wasLooping] {
                    if (!window) return;
                    if (wasLooping) window->refresh_status("STOP");
                    window->update_traffic_graph(0, 0, 0, 0);
                });
                continue;
            }
            looping.store(true, std::memory_order_release);

            // UpdateAll owns QueryStats (outside lock) + apply (under lock).
            UpdateAll();

            QString proxyDisplay;
            QString directDisplay;
            double proxyDownlinkRate = 0;
            double proxyUplinkRate = 0;
            double directDownlinkRate = 0;
            double directUplinkRate = 0;
            QList<int> profileIds;
            {
                QMutexLocker lock(&loop_mutex);
                if (!proxy || !direct) continue;
                proxyDisplay = DisplaySpeed(proxy);
                directDisplay = DisplaySpeed(direct);
                proxyDownlinkRate = proxy->downlink_rate;
                proxyUplinkRate = proxy->uplink_rate;
                directDownlinkRate = direct->downlink_rate;
                directUplinkRate = direct->uplink_rate;
                publishRateSnapshotUnlocked({
                    proxyDownlinkRate,
                    proxyUplinkRate,
                    directDownlinkRate,
                    directUplinkRate,
                });
                // One batched refresh, deduplicated: an auto selector is credited
                // by every one of its members, so a 300-member pool would
                // otherwise fire hundreds of list refreshes every second.
                QSet<int> seen;
                for (const auto& group : groups) {
                    for (const auto& profile : group.profiles) {
                        if (!profile || profile->id < 0) continue;
                        if (seen.contains(profile->id)) continue;
                        seen.insert(profile->id);
                        profileIds.append(profile->id);
                    }
                }
            }

            if (++secs_since_save >= kTrafficSaveIntervalSecs) {
                secs_since_save = 0;
                PersistTraffic();
            }

            const QPointer<MainWindow> window(GetMainWindow());
            runOnUiThread([window, proxyDisplay, directDisplay, proxyDownlinkRate,
                           proxyUplinkRate, directDownlinkRate, directUplinkRate, profileIds] {
                if (!window) return;
                window->refresh_status(QObject::tr("Proxy: %1\nDirect: %2")
                                           .arg(proxyDisplay, directDisplay));
                window->update_traffic_graph(proxyDownlinkRate, proxyUplinkRate,
                                             directDownlinkRate, directUplinkRate);
                if (!profileIds.isEmpty()) {
                    window->refresh_proxy_list(profileIds);
                }
            });
        }
    }

    void TrafficLooper::PersistTraffic() {
        QList<std::shared_ptr<Configs::Profile>> all;
        {
            QMutexLocker lk(&loop_mutex);
            // A profile can appear in several groups (an auto selector is
            // credited by every one of its members), so dedup before writing.
            QSet<int> seen;
            for (auto& group : groups) {
                if (!group.dirty) continue;
                group.dirty = false;
                for (const auto& profile : group.profiles) {
                    if (!profile || profile->id < 0) continue;
                    if (seen.contains(profile->id)) continue;
                    seen.insert(profile->id);
                    all.append(profile);
                }
            }
        }
        if (all.isEmpty()) return;
        if (Configs::dataManager && Configs::dataManager->profilesRepo) {
            Configs::dataManager->profilesRepo->SaveTrafficBatch(all);
        }
    }

    void TrafficLooper::SetChainGroups(const QList<Configs::TrafficChainGroup>& configGroups) {
        QMutexLocker lock(&loop_mutex);
        proxy = std::make_shared<TrafficLooperEntry>();
        proxy->tag = "proxy";
        direct = std::make_shared<TrafficLooperEntry>();
        direct->tag = "direct";

        // Seed last_update to "now" so the first delta lands against the next
        // tick rather than against time zero — otherwise the first rate sample
        // gets divided by however long the app has been up.
        const auto now = elapsed_timer.isValid() ? elapsed_timer.elapsed() : 0;

        groups.clear();
        for (const auto& configGroup : configGroups) {
            if (configGroup.watchTag.isEmpty() || configGroup.profiles.isEmpty()) continue;
            TrafficLooperGroup g;
            g.watchTag = configGroup.watchTag;
            g.profiles = configGroup.profiles;
            g.last_update = now;
            groups.append(g);
        }
        direct_last_update = now;
        publishRateSnapshotUnlocked({});

        // Snapshot reference metadata for the statistics module so per-config
        // history stays meaningful even after a profile is renamed or removed.
        trafficStatsManager->EnsureDirectMeta();
        QSet<int> snapshotted;
        for (const auto& g : groups) {
            for (const auto& p : g.profiles) {
                if (!p || p->id < 0) continue;
                if (snapshotted.contains(p->id)) continue;
                snapshotted.insert(p->id);
                QString groupName;
                if (const auto grp = Configs::dataManager->groupsRepo->GetGroup(p->gid)) groupName = grp->name;
                trafficStatsManager->SnapshotConfigMeta(
                    p->id,
                    p->outbound ? p->outbound->DisplayName() : p->name,
                    groupName,
                    p->type,
                    p->outbound ? p->outbound->DisplayAddress() : QString());
            }
        }
    }

    void TrafficLooper::publishRateSnapshotUnlocked(const TrafficRateSnapshot& snap) {
        QMutexLocker rateLock(&rate_mutex_);
        published_rates_ = snap;
    }

    TrafficRateSnapshot TrafficLooper::GetRateSnapshot() const {
        QMutexLocker rateLock(&rate_mutex_);
        return published_rates_;
    }

} // namespace Stats
