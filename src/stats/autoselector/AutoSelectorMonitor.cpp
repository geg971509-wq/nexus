#include "include/stats/autoselector/AutoSelectorMonitor.hpp"

#include "include/api/RPC.h"
#include "include/global/InterruptibleWait.hpp"
#include "include/ui/mainwindow_interface.h"

#include <QDateTime>
#include <QThread>

namespace Stats
{
    AutoSelectorMonitor *autoSelectorMonitor = new AutoSelectorMonitor;

    namespace
    {
        constexpr int kPollIntervalMs = 2000;

        QString elapsedText(qint64 sinceMs)
        {
            if (sinceMs <= 0) return QObject::tr("never");
            const auto secs = (QDateTime::currentMSecsSinceEpoch() - sinceMs) / 1000;
            if (secs < 60) return QObject::tr("%1s ago").arg(secs);
            if (secs < 3600) return QObject::tr("%1m ago").arg(secs / 60);
            return QObject::tr("%1h ago").arg(secs / 3600);
        }
    } // namespace

    QString AutoSelectorView::summary() const
    {
        if (!valid) return {};
        if (suspended) {
            return QObject::tr("Auto selector paused — no network connection (%1 profiles held)")
                .arg(membersTotal);
        }
        if (phase == "starting" || (phase == "probing" && membersProbed == 0)) {
            return QObject::tr("Auto selector starting — checking %1 profiles").arg(membersTotal);
        }
        if (phase == "probing") {
            return QObject::tr("Auto selector checking profiles (%1/%2 measured)")
                .arg(membersProbed)
                .arg(membersTotal);
        }
        if (membersAlive == 0) {
            return QObject::tr("Auto selector — no working profile out of %1").arg(membersTotal);
        }
        QString base = QObject::tr("Auto selector on %1 (%2 of %3 working)")
                           .arg(selectedName.isEmpty() ? selectedTag : selectedName)
                           .arg(membersAlive)
                           .arg(membersTotal);
        if (lastSwitchMs > 0) base += QObject::tr(", switched %1").arg(elapsedText(lastSwitchMs));
        return base;
    }

    QString AutoSelectorView::detail() const
    {
        if (!valid) return {};
        QStringList parts;
        if (membersProbed > 0) {
            parts << QObject::tr("%1 working").arg(membersAlive);
            if (membersCooldown > 0) parts << QObject::tr("%1 cooling down").arg(membersCooldown);
            const int untested = membersTotal - membersProbed;
            if (untested > 0) parts << QObject::tr("%1 not checked yet").arg(untested);
        }
        if (probesInFlight > 0) parts << QObject::tr("%1 being checked").arg(probesInFlight);
        if (balance) parts << QObject::tr("balancing over %1").arg(membersQualified);
        if (parts.isEmpty()) return {};
        return QObject::tr("%1 profiles: %2").arg(membersTotal).arg(parts.join(QObject::tr(", ")));
    }

    void AutoSelectorMonitor::SetBuild(const QList<Configs::AutoSelectorBuildInfo> &infos)
    {
        QMutexLocker lock(&mutex);
        tagToProfile.clear();
        tagToName.clear();
        view = AutoSelectorView{};
        exhaustedSince = 0;
        groupTag.clear();
        profileID = -1;
        active = false;
        if (infos.isEmpty()) return;

        // Throne only ever builds one selector per config (it is the profile
        // being started); take the first and ignore any future extras.
        const auto &info = infos.first();
        groupTag = info.groupTag;
        profileID = info.profile ? info.profile->id : -1;
        for (const auto &[tag, member] : info.members) {
            if (member == nullptr) continue;
            tagToProfile.insert(tag, member->id);
            tagToName.insert(tag, member->outbound ? member->outbound->DisplayName() : member->name);
        }
        view.valid = true;
        view.groupTag = groupTag;
        view.profileID = profileID;
        view.phase = "starting";
        view.membersTotal = static_cast<int>(info.members.size());
        active = true;
    }

    void AutoSelectorMonitor::Clear()
    {
        QMutexLocker lock(&mutex);
        active = false;
        view = AutoSelectorView{};
        tagToProfile.clear();
        tagToName.clear();
        exhaustedSince = 0;
        groupTag.clear();
        profileID = -1;
    }

    bool AutoSelectorMonitor::Active() const
    {
        QMutexLocker lock(&mutex);
        return active;
    }

    AutoSelectorView AutoSelectorMonitor::Snapshot() const
    {
        QMutexLocker lock(&mutex);
        return view;
    }

    void AutoSelectorMonitor::RequestRecheck() const
    {
        QString tag;
        {
            QMutexLocker lock(&mutex);
            if (!active) return;
            tag = groupTag;
        }
        bool ok = false;
        API::defaultClient->AutoSelectorAction(&ok, tag, "recheck");
    }

    void AutoSelectorMonitor::Start()
    {
        worker.Start([this](std::stop_token stopToken) { Loop(stopToken); });
    }

    void AutoSelectorMonitor::Stop()
    {
        worker.Stop([this] {
            QMutexLocker lock(&wait_mutex);
            wait_condition.wakeAll();
        });
    }

    void AutoSelectorMonitor::Loop(std::stop_token stopToken)
    {
        while (!stopToken.stop_requested()) {
            if (!Throne::waitForStopOrTimeout(wait_condition, wait_mutex, stopToken, kPollIntervalMs)) break;
            if (!Active()) continue;
            poll();
        }
    }

    void AutoSelectorMonitor::poll()
    {
        bool ok = false;
        const auto resp = API::defaultClient->QueryAutoSelectors(&ok);
        if (!ok) return;

        QString wantedTag;
        {
            QMutexLocker lock(&mutex);
            if (!active) return;
            wantedTag = groupTag;
        }

        const libcore::AutoSelectorStatus *status = nullptr;
        for (const auto &group : resp.groups) {
            if (QString::fromStdString(group.tag.value()) == wantedTag) {
                status = &group;
                break;
            }
        }
        if (status == nullptr) return;

        AutoSelectorView next;
        next.valid = true;
        next.groupTag = wantedTag;
        next.phase = QString::fromStdString(status->phase.value());
        next.selectedTag = QString::fromStdString(status->selected.value());
        next.balance = status->balance.value();
        next.balanceMode = QString::fromStdString(status->balance_mode.value());
        next.suspended = status->suspended.value();
        next.suspendedSinceMs = status->suspended_since_ms.value();
        next.membersTotal = status->members_total.value();
        next.membersProbed = status->members_probed.value();
        next.membersAlive = status->members_alive.value();
        next.membersQualified = status->members_qualified.value();
        next.membersCooldown = status->members_cooldown.value();
        next.probesInFlight = status->probes_in_flight.value();
        next.roundsCompleted = status->rounds_completed.value();
        next.lastRoundMs = status->last_round_ms.value();
        next.nextRoundMs = status->next_round_ms.value();
        next.lastSwitchMs = status->last_switch_ms.value();
        next.lastSwitchReason = QString::fromStdString(status->last_switch_reason.value());
        next.updatedAtMs = QDateTime::currentMSecsSinceEpoch();

        int usable = 0;
        {
            QMutexLocker lock(&mutex);
            next.profileID = profileID;
            for (const auto &member : status->members) {
                AutoSelectorMemberView entry;
                entry.tag = QString::fromStdString(member.tag.value());
                entry.profileID = tagToProfile.value(entry.tag, -1);
                entry.name = tagToName.value(entry.tag);
                entry.rank = member.rank.value();
                entry.state = QString::fromStdString(member.state.value());
                entry.selected = member.selected.value();
                entry.selectedUDP = member.selected_udp.value();
                entry.qualified = member.qualified.value();
                entry.active = member.active.value();
                entry.averageMs = member.average_ms.value();
                entry.deviationMs = member.deviation_ms.value();
                entry.minMs = member.min_ms.value();
                entry.maxMs = member.max_ms.value();
                entry.samples = member.samples.value();
                entry.failures = member.failures.value();
                entry.probes = member.probes.value();
                entry.dialTotal = member.dial_total.value();
                entry.dialFail = member.dial_fail.value();
                entry.lastOKms = member.last_ok_ms.value();
                entry.lastProbeMs = member.last_probe_ms.value();
                entry.cooldownUntilMs = member.cooldown_until_ms.value();
                entry.lastError = QString::fromStdString(member.last_error.value());
                if (entry.selected) {
                    next.selectedName = entry.name;
                    next.selectedProfileID = entry.profileID;
                }
                // "untested" counts as usable: it has not failed, the prober
                // simply has not reached it yet.
                if (entry.isUsable() || entry.state == "untested") usable++;
                next.members.append(entry);
            }
        }

        const auto now = QDateTime::currentMSecsSinceEpoch();
        bool fireExhausted = false;

        {
            QMutexLocker lock(&mutex);
            if (!active) return;

            // The pool only counts as exhausted when the core itself is sure the
            // local network is fine — a dropped wifi suspends the core, and a
            // suspended core must never trigger a rebuild.
            const bool everythingDead = usable == 0 && next.membersProbed > 0 && !next.suspended
                && next.phase != "starting";
            if (!everythingDead) {
                exhaustedSince = 0;
            } else if (exhaustedSince == 0) {
                exhaustedSince = now;
            }
            next.exhaustedSinceMs = exhaustedSince;

            if (exhaustedSince != 0 && (now - exhaustedSince) >= kPoolExhaustedGraceSecs * 1000) {
                const auto backoffMs = static_cast<qint64>(rebuildBackoffSecs) * 1000;
                if (lastRebuildRequest == 0 || (now - lastRebuildRequest) >= backoffMs) {
                    lastRebuildRequest = now;
                    rebuildBackoffSecs = rebuildBackoffSecs == 0
                                             ? kRebuildBackoffMinSecs
                                             : std::min(rebuildBackoffSecs * 2, kRebuildBackoffMaxSecs);
                    exhaustedSince = 0;
                    fireExhausted = true;
                }
            }
            if (usable > 0) {
                // A working pool resets the ladder, so an unrelated failure
                // later is not punished by an old backoff.
                rebuildBackoffSecs = 0;
            }
            view = next;
        }

        emit updated();
        if (fireExhausted) {
            // Second, independent check: the OS default route. If this machine
            // has no network at all, every profile "failing" says nothing about
            // the profiles, and rebuilding would only burn the ranked pool.
            bool interfaceOK = false;
            const auto iface = API::defaultClient->GetDefaultInterface(&interfaceOK);
            if (interfaceOK && iface.name.value().empty()) {
                MW_show_log(QObject::tr("[Auto selector] Every profile is failing, but this machine has no "
                                        "network connection — keeping the current pool."));
                return;
            }
            emit poolExhausted(next.profileID);
        }
    }
} // namespace Stats
