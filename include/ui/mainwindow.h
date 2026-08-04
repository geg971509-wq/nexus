#pragma once

#include <QMainWindow>
#include <include/global/HTTPRequestHelper.hpp>
#ifndef Q_MOC_RUN
#include <core/server/gen/libcore.pb.h>
#endif

#include "include/global/Configs.hpp"
#include "include/sys/Process.hpp"
#include "include/stats/connections/connectionLister.hpp"
#include "3rdparty/qv2ray/v2/ui/widgets/speedchart/SpeedWidget.hpp"
#include "include/database/entities/Profile.h"
#ifdef Q_OS_LINUX
#include <QtDBus>
#endif

#ifndef MW_INTERFACE

#include <QKeyEvent>
#include <QIcon>
#include <QPixmap>
#include <QSystemTrayIcon>
#include <QPointer>
#include <QTimer>
#include <QQueue>
#include <QWaitCondition>
#include <QProcess>
#include <QTextDocument>
#include <QShortcut>
#include <QKeySequence>
#include <QSet>
#include <QCheckBox>
#include <QSemaphore>
#include <QMutex>
#include <QThreadPool>
#include <QLocalServer>
#include <QLocalSocket>

#include <atomic>
#include <memory>

#include "include/database/entities/GroupSort.hpp"
#include "include/global/GuiUtils.hpp"
#include "include/ui/utils/OperationGate.h"
#include "include/ui/utils/DataViewHtmlGenerator.h"
#include "include/ui/utils/ProfilesFilterProxyModel.h"
#include "include/ui/utils/ProfilesTableModel.h"
#include "ui_mainwindow.h"

#endif

namespace Configs_sys {
    class CoreProcess;
}

class QAction;
class QMenu;
class QToolButton;
class TrayProfileSelector;

namespace Qv2ray::ui { class SyntaxHighlighter; }

QT_BEGIN_NAMESPACE
namespace Ui {
    class MainWindow;
}
QT_END_NAMESPACE

enum class RefreshAnchor {
    // Re-select the same profiles by id; select nothing if they are gone.
    KeepPlace,
    // As above, but if all of them were deleted select whatever took their row.
    Removal,
};

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

    ~MainWindow() override;

    // Runtime Stats panel helpers, read on the UI thread. GetCorePid returns 0
    // when the core process isn't running; GetRunningConfigName is empty when no
    // profile is active.
    qint64 GetCorePid();
    QString GetRunningConfigName();

    void prepare_exit();

    void refresh_proxy_list(const QList<int> &ids = {}, bool mayNeedReset = false,
                            RefreshAnchor anchor = RefreshAnchor::KeepPlace);

    void show_group(int gid);

    void refresh_groups();

    void refresh_status(const QString &traffic_update = "");

    void update_traffic_graph(int proxyDl, int proxyUp, int directDl, int directUp);

    void profile_start(int _id = -1);

    bool profile_stop(bool crash = false, bool block = false, bool manual = false);

    int get_profile_to_start();

    void set_spmode_system_proxy(bool enable, bool save = true);

    void toggle_system_proxy();

    void set_spmode_vpn(bool enable, bool save = true);

    bool get_elevated_permissions(int reason = 3);

    void start_select_mode(QObject *context, const std::function<void(int)> &callback);

    void RegisterHotkey(bool unregister);

    bool StopVPNProcess();

    void UpdateConnectionList(const QMap<QString, Stats::ConnectionMetadata>& toUpdate, const QMap<QString, Stats::ConnectionMetadata>& toAdd);

    void UpdateConnectionListWithRecreate(const QList<Stats::ConnectionMetadata>& connections);

    void UpdateDataView(bool force = false);

    // Pushes the auto-selector snapshot into the data view, toggles the Tools
    // entry, and refreshes the dialog if it is open.
    void refresh_auto_selector_view();

    // Non-owning: cleared by the dialog's finished() handler.
    class DialogAutoSelector *m_autoSelectorDialog = nullptr;

    void setDownloadReport(const DownloadProgressReport& report, bool show);

signals:

    void profile_selected(int id);

public slots:

    void on_commitDataRequest();

    void on_menu_exit_triggered();

#ifndef MW_INTERFACE

private slots:

    void on_masterLogBrowser_customContextMenuRequested(const QPoint &pos);

    void on_menu_basic_settings_triggered();

    void on_menu_routing_settings_triggered();

    void on_menu_vpn_settings_triggered();

    void on_menu_hotkey_settings_triggered();

    void on_menu_add_from_input_triggered();

    void on_menu_add_from_clipboard_triggered();

    void on_menu_clone_triggered();

    void on_menu_delete_repeat_triggered();

    void on_menu_delete_triggered();

    void on_menu_reset_traffic_triggered();

    void on_menu_copy_links_triggered();

    void on_menu_copy_links_nkr_triggered();

    void on_menu_export_config_triggered();

    void display_qr_link(bool nkrFormat = false);

    void on_menu_scan_qr_triggered();

    void on_menu_clear_test_result_triggered();

    void on_menu_manage_groups_triggered();

    void on_menu_select_all_triggered();

    void on_menu_remove_unavailable_triggered();

    void on_menu_remove_invalid_triggered();

    void on_menu_remove_insecure_triggered();

    void on_menu_resolve_selected_triggered();

    void on_menu_resolve_domain_triggered();

    void on_menu_update_subscription_triggered();

    void on_profilesTableView_doubleClicked(const QModelIndex &index);

    void on_profilesTableView_customContextMenuRequested(const QPoint &pos);

    void on_tabWidget_currentChanged(int index);

    void on_tabWidget_customContextMenuRequested(const QPoint& p);

private:
    void setupCore();
    void setupProfileView();
    void setupTray();
    void setupActions();
    void setupPeriodicJobs();

    Ui::MainWindow *ui;
    ProfilesTableModel *profilesTableModel = nullptr;
    // What the view is attached to: rows from the view or its selection model are
    // proxy rows, not profilesTableModel rows.
    ProfilesFilterProxyModel *profilesFilterModel = nullptr;
    QSystemTrayIcon *tray = nullptr;
    QMenu *trayMenu = nullptr;    // tray context menu
    QToolButton *filterButton = nullptr;
    QAction *traySelectServerAction = nullptr;
    QAction *traySelectRoutingAction = nullptr;
    QMenu *traySpmodeMenu = nullptr;
    // Tray "Select Server"/"Select Routing" open this small Qt-drawn popup instead of a
    // submenu, because a tray submenu isn't painted by Qt on Linux (SNI/DBusMenu) or macOS
    // (native NSMenu) and so can't reliably expand a dynamic list. Recreated on each open.
    QPointer<TrayProfileSelector> traySelector;
    void openTraySelector(bool routing);
    QShortcut *shortcut_esc = new QShortcut(QKeySequence::Cancel, this);
    //
    QThreadPool *parallelCoreCallPool = new QThreadPool(this);
    QThreadPool *operationCallPool = new QThreadPool(this);
    std::atomic<bool> acceptingOperations_ = true;
    std::atomic<bool> stopSpeedtest = false;
    Throne::OperationGate speedtestOperation_;
    std::atomic<bool> currentUnderTest = false;
    // Speed-test byte accounting. Tests bypass the clash tracker (they dial the
    // outbound directly), so their traffic is counted only here: the core reports
    // each test's cumulative bytes, and we diff against the last reported value
    // per outbound tag to credit the delta. Guarded so the live micro-poll and
    // the final reconciliation pass don't race.
    QMutex speedtestCreditMu_;
    QHash<QString, QPair<qint64, qint64>> speedtestCredited_;
    //
    std::unique_ptr<Configs_sys::CoreProcess> core_process;
    QMutex coreProcessMutex; // serializes core_process init (DS_cores) vs IPC newConnection (UI)
    QLocalServer *core_server = nullptr;
    qint64 core_pid = 0;
    qint64 launched_core_pid = 0;
    quint64 core_connection_generation = 0;
    // Rate-limit pre-IPC detached-core relaunches (ms since epoch; 0 = never).
    qint64 last_pre_ipc_relaunch_ms = 0;
    bool rpc_started = false;
    qint64 vpn_pid = 0;
    //
    QTextDocument *qvLogDocument = new QTextDocument(this);
    //
    QString title_error;
    int icon_status = -1;
    // Read on pool threads (mainwindow_rpc.cpp) and written on the UI thread;
    // all access goes through std::atomic_load(&running)/std::atomic_store.
    std::shared_ptr<Configs::Profile> running;
    int last_running_profile_id = -1;
    // True from the moment a profile start is kicked off until it succeeds or
    // fails; drives the start/stop button's transient "Connecting" state.
    bool m_profileConnecting = false;
    // Set from UI while Connecting; start worker checks this and aborts.
    std::atomic_bool m_profileStartCancelRequested{false};
    // True while a profile stop is in progress; drives the "Disconnecting" state.
    bool m_profileDisconnecting = false;
    // Single-flight guard for the Xray geo-asset (geoip.dat/geosite.dat) download
    // prompt: a batch test can surface the missing-asset error for many profiles at
    // once, and we only want one prompt/download. Touched on the UI thread only.
    bool m_xrayGeoAssetBusy = false;
    // Single-flight guard shared by subscription updates and domain resolution, so
    // one long refresh cannot be stacked on another. Cleared from the completion
    // callback, which GroupUpdater marshals back to the UI thread.
    bool m_subUpdating = false;
    // Single-flight guard for the modeless settings dialogs (showUniqueDialog).
    bool m_dialogGuard = false;
    // Serialize elevated up/down (toolbar + proxy lifecycle share one tunnel).
    std::atomic_bool warpOpBusy{false};
    // Desired on/off while busy (checkbox must not lag the real process).
    std::atomic_bool warpDesiredOn{false};
    std::atomic_bool warpStatusPollInFlight{false};
    std::atomic<quint64> warpStatusGeneration{0};
    QTimer *warpStatusTimer = nullptr;
    Configs_sys::WarpStatus warpRuntimeStatus = Configs_sys::WarpStatus::Unknown;
    QString warpTransport;
    // Last underlay device name from the 2s status poll (or Up/Down result).
    // Config generation reads this instead of forking warp-client on the UI thread.
    QString warpInterfaceName;
    // WARP device the running core's config pinned as route.default_interface,
    // empty when the running config does not depend on the underlay. Egress is
    // bound to that device, so if it disappears the config must be rebuilt --
    // otherwise every dial targets a dead interface.
    QString warpInterfaceInRunningConfig;
    QString cachedWarpUnderlayForConfig() const;
    void refreshWarpRuntimeStatus();
    void refreshWarpButton();
    bool acquireWarpOp(int waitMs = 0);
    void releaseWarpOp();
    // canceled lets the readiness wait give up early; without it a user abort
    // cannot land until the full WARP timeout elapses.
    bool ensureWarpReady(bool *startedThisAttempt, QString *error,
                         const std::function<bool()> &canceled = {});
    bool askContinueWithoutWarp(const QString &error);
    // Unified WARP toggle: starts/stops the system tunnel and persists enable_warp.
    bool setWarpEnabled(bool enable);
    QString traffic_update_cache;
    qint64 last_test_time = 0;
    //
    int proxy_last_order = -1;
    bool select_mode = false;
    Throne::OperationGate profileOperation_;
    QMutex mu_exit;
    int exit_reason = 0;
    //
    QMutex mu_download_update;
    //
    QMutex connectionListMu;
    //
    int toolTipID = 0;
    //
    SpeedWidget *speedChartWidget;
    //
    // for data view
    QMutex dataViewMutex_;
    QDateTime lastUpdated = QDateTime::currentDateTime();
    DataViewHtmlGenerator dataViewHtmlGenerator_;

    // shortcuts
    QList<QShortcut*> hiddenMenuShortcuts;

    // search
    QString addressFilterString;
    QString nameFilterString;
    QString typeFilterString;
    QString countryFilterString;

    QTimer *m_filterRefreshDebounce = nullptr;

    // Only meaningful between a saveProfileFocusState() and its restore.
    bool m_profilesTableHadFocus = false;
    int m_profilesScrollValue = 0;

    // log
    QStringList includeKeywords;
    QStringList excludeKeywords;
    QRegularExpression includeCombined;
    QRegularExpression excludeCombined;
    QMutex logMutex;
    QQueue<QString> logQueue;
    QWaitCondition logWaiter;
    std::atomic<bool> logProcessorStopping_{false};
    Qv2ray::ui::SyntaxHighlighter *logHighlighter = nullptr;

    // Immutable snapshot of the log filter fields. The log thread copies these
    // under logMutex (Qt containers are copy-on-write, so it's O(1)) and then
    // filters without holding the lock, so producers calling append_log() are
    // never blocked on the regex/keyword work.
    struct LogFilter {
        bool enableInclude = false;
        bool enableExclude = false;
        QStringList includeKeywords;
        QStringList excludeKeywords;
        QRegularExpression includeCombined;
        QRegularExpression excludeCombined;
    };

    void append_log(const QString &log);

    void log_process_loop();

    void stopLogProcessor();

    void stopCoreDispatcher();

    bool should_print_log(const QString &log, const LogFilter &filter);

    void updateLogFilterFields();

    // (Re)installs the log syntax highlighter, deleting any previous one so
    // highlighters don't stack up (and keep re-highlighting) on theme changes.
    void setLogHighlighter(bool darkMode);

    void applyProfileFilters();

    QList<int> get_now_selected_list();
    void refresh_startstop_button();

    QList<int> get_selected_or_group();

    bool set_system_proxy(bool enable);

    void saveProfileFocusState();

    void restoreProfileFocusState(RefreshAnchor anchor);

    void selectProfileRows(const QList<int> &rows);

    void focusProfilesTable(bool selectFirst);

    void clearUnavailableProfiles(bool confirm = true, QList<int> profileIDs = {});
    void resolveProfilesToIP(const QList<int> &profileIDs);

    // Defined in mainwindow.cpp, the only translation unit that knows the dialogs.
    template<class Dialog, class... Args>
    void showUniqueDialog(Args &&... args);

    void dialog_message_impl(MwMessage cmd, const QStringList &args);

    void handle_deeplink_impl(const QString &url);

    void handle_addsub(const QString &url, const QString &name);

    void handle_import_route(const QString &url);

    // throne://remoteRoute?data=<...> : add one or more remote routing profiles. The data is
    // (base64 of) a JSON array of {url, auto_update[, name]} objects.
    void handle_add_remote_routes(const QString &url);

    // Routes user-supplied text: throne:// links go to the deeplink handler, the
    // rest to the subscription/profile importer.
    void import_or_handle_deeplink(const QString &text);

    void refresh_proxy_list_column_size();

    void refresh_proxy_list_impl(const QList<int> &ids = {}, bool mayNeedReset = false);

    void refresh_proxy_list_impl_refresh_data(const QList<int>& ids = {}, bool mayNeedReset = false);

    void parseQrImage(const QPixmap *image);

    void keyPressEvent(QKeyEvent *event) override;

    void closeEvent(QCloseEvent *event) override;

    void changeEvent(QEvent *event) override;

    void showEvent(QShowEvent *event) override;

    void hideEvent(QHideEvent *event) override;

    void resizeEvent(QResizeEvent *event) override;

    // Tell the connection lister whether its tab is actually on screen (stats tab
    // selected, window neither minimized nor hidden to tray) so it can drop to a
    // relaxed poll cadence when nobody is looking. Recomputed on tab/visibility
    // changes.
    void syncConnectionViewState();

    void dragEnterEvent(QDragEnterEvent *event) override;

    void dropEvent(QDropEvent* event) override;

    void applyLogBrowserFont();

    // Re-derives the top bar's sizing from the current font and translation, and
    // raises the window's minimum to whatever the layout actually needs. Called
    // at startup and on every font change.
    void applyTopBarMetrics();

    // The window minimum the .ui was designed with; applyTopBarMetrics() only ever
    // grows past this, so a smaller font returns to the designed floor.
    QSize designMinimumSize;

    // Debounced refresh_proxy_list trigger for font/theme/resize events.
    QTimer *m_proxyListRefreshDebounce = nullptr;
    void scheduleProxyListRefresh();

    // Spin the globe tray icon while system WARP is the active tray status.
    QTimer *m_warpTraySpinTimer = nullptr;
    qreal m_warpTrayAngle = 0;
    QPixmap m_warpTrayBase;
    void updateWarpTraySpin(bool spinning);
    QIcon warpTrayIconAtAngle(qreal angle) const;

    bool m_adjustingColumns = false;

    //

    void HotkeyEvent(const QString &key);

    void RegisterHiddenMenuShortcuts(bool unregister = false);
    // Register a QShortcut for every action in `menu` (recursing into submenus),
    // appending them to hiddenMenuShortcuts. Needed because the menubar is hidden,
    // so actions reachable only through popup menus get no shortcut on their own.
    // `claimed` holds the key sequences already handled (either by Qt automatically
    // or by an earlier call); shortcuts already in it are skipped to avoid the
    // ambiguous-shortcut conflict that breaks actions shared with other menus.
    void registerMenuShortcuts(QMenu *menu, QSet<QKeySequence> &claimed);
    // Collect the shortcut key sequences of every action in `menu` (recursing into
    // submenus) into `out`, without registering anything.
    void collectMenuShortcuts(QMenu *menu, QSet<QKeySequence> &out);

    void setActionsData();

    QList<QAction*> getActionsForShortcut();

    void loadShortcuts();

    // rpc

    void setup_rpc(QLocalSocket *socket);

    qint64 verified_core_pid(QLocalSocket *socket, qint64 expectedPid = 0);

    // Detached Start() + pre-IPC death watch. Call Start on DS_cores, arm watch on UI.
    bool startCoreDetached();
    void armPreIpcWatch(qint64 watchPid, quint64 watchGeneration);
    void launchCoreProcess();

    enum class ProfileStopAdmission {
        Normal,
        Shutdown,
    };
    bool profile_stop(bool crash, bool block, bool manual, ProfileStopAdmission admission);
    bool profile_stop_impl(bool crash, bool manual, int id);

    void urltest_current_group(const QList<int>& profileIDs);

    // Measures the members of an auto selector that have no test result yet
    // (plus `stale`, whose stored result is known to be out of date) and
    // rewrites its ranked pool. Blocks — call from a worker thread.
    void rank_auto_selector(const std::shared_ptr<Configs::Profile>& ent, const QList<int>& stale = {});

    // Every running member of the auto selector died: re-rank and restart on
    // the next batch of good ones.
    void on_auto_selector_exhausted(int profileID);

    // A subscription refresh rewrote the servers of `gid`. Drops ids that no
    // longer exist from every selector tracking that group, and rebuilds the
    // running one only if the refresh touched a member it actually built.
    // `disturbed` holds the profiles the refresh deleted or replaced in place.
    void on_subscription_group_changed(int gid, const QList<int>& disturbed);

    // Guards the re-entrant profile_start used to rank before building.
    bool auto_selector_ranked = false;

    void iptest_current_group(const QList<int>& profileIDs);

    void stopTests();
    // Acquire speedtestOperation_ or show Wait / Stop when another test is still running.
    bool beginOrPromptBusyTest();

    void runURLTest(const QString& config, const QString& xrayConfig, const QStringList& xrayFullConfigs, bool useDefault, const QStringList& outboundTags, const QMap<QString, int>& tag2entID, int entID = -1);

    void runIPTest(const QString& config, const QString& xrayConfig, const QStringList& xrayFullConfigs, bool useDefault, const QStringList& outboundTags, const QMap<QString, int>& tag2entID, int entID = -1);

    // If `error` reports missing Xray geo assets (geoip.dat / geosite.dat), prompt
    // once (guarded by m_xrayGeoAssetBusy) and download the missing .dat files in
    // the background. Shared by profile start and the test paths. `contextName` is
    // the profile/config name shown in the prompt. Returns true when the error was
    // a geo-asset error (and thus handled), false otherwise.
    bool handleXrayGeoAssetError(const QString& error, const QString& contextName);

    void url_test_current();

    void speedtest_current_group(const QList<int>& profileIDs, bool testCurrent = false);

    void runSpeedTest(const QString& config, const QString& xrayConfig, const QStringList& xrayFullConfigs, bool useDefault, bool testCurrent, const QStringList& outboundTags, const QMap<QString, int>& tag2entID, int entID = -1);

    bool set_system_dns(bool set, bool save_set = true);

    void CheckUpdate();

    void setupConnectionList();

    void setupConnectionSortMenu();

    void querySpeedtest(const QMap<QString, int>& tag2entID, bool testCurrent);

    // Credit the delta between a test's cumulative bytes (curUp/curDown) and the
    // last reported values for `tag`. Feeds the time-series stats (the tested
    // config + a synthetic "Speedtest" app) and the legacy per-profile total.
    // Speed tests bypass the clash tracker, so the looper never sees these bytes;
    // this is the only place they are counted, for both a selected-profile test
    // and a current-instance test.
    void creditSpeedtestTraffic(const std::shared_ptr<Configs::Profile>& profile, const QString& tag, qint64 curUp, qint64 curDown);

    void queryCountryTest(const QMap<QString, int>& tag2entID, bool testCurrent);

protected:
    bool eventFilter(QObject *obj, QEvent *event) override;

#endif // MW_INTERFACE
};

inline MainWindow *GetMainWindow() {
    // qobject_cast nulls a destroyed/wrong-type object instead of returning a
    // dangling C-style cast. Callers that hop threads still need QPointer.
    return qobject_cast<MainWindow *>(mainwindow);
}

void UI_InitMainWindow();

#ifdef Q_OS_LINUX
/*
 * Proxy class for interface org.freedesktop.portal.Request
 */
class OrgFreedesktopPortalRequestInterface : public QDBusAbstractInterface
{
    Q_OBJECT
public:
    OrgFreedesktopPortalRequestInterface(const QString& service,
                                         const QString& path,
                                         const QDBusConnection& connection,
                                         QObject* parent = nullptr);

    ~OrgFreedesktopPortalRequestInterface();

public Q_SLOTS:
    inline QDBusPendingReply<> Close()
    {
        QList<QVariant> argumentList;
        return asyncCallWithArgumentList(QStringLiteral("Close"), argumentList);
    }

Q_SIGNALS: // SIGNALS
    void Response(uint response, QVariantMap results);
};

namespace org {
namespace freedesktop {
namespace portal {
typedef ::OrgFreedesktopPortalRequestInterface Request;
}
}
}
#endif
