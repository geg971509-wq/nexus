#include <csignal>

#include <QApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFileInfo>
#include <QTranslator>
#include <QMessageBox>
#include <QStandardPaths>
#include <QLocalSocket>
#include <QLocalServer>
#include <QThread>
#include <QDateTime>
#include <QTimer>
#include <3rdparty/WinCommander.hpp>


#include "include/global/Configs.hpp"
#include "include/ui/setting/BackupRestore.h"

#include "include/ui/mainwindow_interface.h"
#include "include/stats/traffic/TrafficStatsManager.hpp"
#include "include/api/RPC.h"

#ifdef Q_OS_WIN
#include "include/sys/windows/MiniDump.h"
#include "include/sys/windows/eventHandler.h"
#include "include/sys/windows/WinVersion.h"
#include <qfontdatabase.h>
#endif
#ifdef Q_OS_LINUX
#include <include/sys/linux/coreDump.h>
#include <qfontdatabase.h>
#endif
#ifdef Q_OS_MACOS
#include <QFileOpenEvent>
#include <unistd.h>

// On macOS the OS reuses the running app and delivers throne:// URLs as a
// QFileOpenEvent to the application object (never via argv). This filter feeds
// them into the common deeplink pipeline.
class MacDeeplinkFilter : public QObject {
public:
    using QObject::QObject;

protected:
    bool eventFilter(QObject *obj, QEvent *event) override {
        if (event->type() == QEvent::FileOpen) {
            const QString url = static_cast<QFileOpenEvent *>(event)->url().toString();
            if (url.startsWith("throne://")) {
                Deeplink_Submit(url);
                return true;
            }
        }
        return QObject::eventFilter(obj, event);
    }
};
#endif

#ifdef Q_OS_LINUX
namespace {
volatile std::sig_atomic_t terminationRequested = 0;

void signal_handler(int) {
    terminationRequested = 1;
}
}
#else
void signal_handler(int) {
    GetMainWindow()->prepare_exit();
    qApp->quit();
}
#endif

static QTranslator *trans = nullptr;

static QString localeForLanguage(int language) {
    switch (language) {
    case 1:
        return QStringLiteral("en_US");
    case 2:
        return QStringLiteral("zh_CN");
    case 3:
        return QStringLiteral("fa_IR");
    case 4:
        return QStringLiteral("ru_RU");
    default:
        return QLocale::system().name();
    }
}

void loadTranslate(int language) {
    // Keep these strings in the translation catalog for native dialogs.
    (void)QT_TRANSLATE_NOOP("QPlatformTheme", "Cancel");
    (void)QT_TRANSLATE_NOOP("QPlatformTheme", "Apply");
    (void)QT_TRANSLATE_NOOP("QPlatformTheme", "Yes");
    (void)QT_TRANSLATE_NOOP("QPlatformTheme", "No");
    (void)QT_TRANSLATE_NOOP("QPlatformTheme", "OK");

    const QLocale locale(localeForLanguage(language));
    QLocale::setDefault(locale);

    if (trans != nullptr) {
        QCoreApplication::removeTranslator(trans);
        delete trans;
        trans = nullptr;
    }

    if (language != 1) {
        // A .qm next to the binary wins over the bundled one, so a translation
        // can be tested without rebuilding the resources.
        const QString diskPath = QCoreApplication::applicationDirPath()
                                 + QStringLiteral("/translations/") + locale.name() + QStringLiteral(".qm");
        const QString qrcPath = QStringLiteral(":/translations/") + locale.name() + QStringLiteral(".qm");
        auto *next = new QTranslator;
        const bool loaded = (QFileInfo::exists(diskPath) && next->load(diskPath)) || next->load(qrcPath);
        if (loaded) {
            trans = next;
            QCoreApplication::installTranslator(trans);
        } else {
            delete next;
        }
    }

    qApp->setLayoutDirection(locale.textDirection());
}

#define LOCAL_SERVER_PREFIX "throne-"

int main(int argc, char* argv[]) {
    // Core dump
#ifdef Q_OS_WIN
    Windows_SetCrashHandler();
#endif
#ifdef Q_OS_LINUX
    enable_core_dumps();
#endif

    QApplication::setAttribute(Qt::AA_DontUseNativeDialogs);
    QApplication::setQuitOnLastWindowClosed(false);
    QApplication a(argc, argv);

#ifdef Q_OS_MACOS
    // Install before the event loop so launch-by-deeplink FileOpen events are caught.
    a.installEventFilter(new MacDeeplinkFilter(&a));
#endif

#if !defined(Q_OS_MACOS) && (QT_VERSION >= QT_VERSION_CHECK(6,9,0))
    // Load the emoji fonts
#ifdef Q_OS_WIN
    int fontId = QFontDatabase::addApplicationFont(WinVersion::IsBuildNumGreaterOrEqual(BuildNumber::Windows_11_22H2) ? ":/font/notoEmoji" : ":/font/Twemoji");
#else
    int fontId = QFontDatabase::addApplicationFont(":/font/notoEmoji");
#endif
    if (fontId >= 0)
    {
        QStringList fontFamilies = QFontDatabase::applicationFontFamilies(fontId);
        QFontDatabase::setApplicationEmojiFontFamilies(fontFamilies);
    } else
    {
        qDebug() << "could not load emoji font!";
    }
#endif

    // Clean
    QDir::setCurrent(QApplication::applicationDirPath());
    if (QFile::exists("updater.old")) {
        QFile::remove("updater.old");
    }

    QStringList arguments = QApplication::arguments();
    const bool waitForRestart = arguments.removeAll("-wait-for-restart") > 0;
    // A throne:// URL may be passed as a launch argument (Windows/Linux). Delivered
    // after the window is up, or forwarded to the primary instance via the socket below.
    const QString launchDeeplink = Deeplink_ExtractFromArgs(arguments);

    // dirs & clean
    auto wd = QDir(QApplication::applicationDirPath());
    bool useAppdata = false;
    QString appdataDir;
    if (arguments.contains("-appdata")) {
        useAppdata = true;
        int appdataIndex = arguments.indexOf("-appdata");
        if (arguments.size() > appdataIndex + 1 && !arguments.at(appdataIndex + 1).startsWith("-")) {
            appdataDir = arguments.at(appdataIndex + 1);
        }
    }
#ifdef NKR_CPP_USE_APPDATA
    useAppdata = true; // Example: Package & MacOS
#endif
    if(useAppdata) {
        QApplication::setApplicationName("Throne");
        if (!appdataDir.isEmpty()) {
            wd.setPath(appdataDir);
        } else {
            wd.setPath(QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation));
        }
    }
    if (!wd.exists()) wd.mkpath(wd.absolutePath());
    if (!wd.exists("config")) wd.mkdir("config");
    QDir::setCurrent(wd.absoluteFilePath("config"));
    QDir("temp").removeRecursively();

    // Record app start for the Runtime Stats uptime readout.
    appStartEpoch = QDateTime::currentSecsSinceEpoch();

    // Recover a restore interrupted after the database or icons were swapped.
    // This must run before SQLite opens the live database and recreates WAL files.
    const QString databasePath = QDir::current().filePath("throne.db");
    QString restoreRecoveryError;
    if (!BackupRestore::recoverPending(QDir::currentPath(), databasePath,
                                       &restoreRecoveryError)) {
        QMessageBox::critical(nullptr, QObject::tr("Restore Recovery Failed"),
            QObject::tr("Throne could not recover an interrupted backup restore:\n%1")
                .arg(restoreRecoveryError));
        return 1;
    }

    // Load database
    try {
        Configs::initDB(databasePath.toStdString());
    } catch (const std::exception& e) {
        QMessageBox::critical(nullptr, QObject::tr("Database Error"),
            QObject::tr("Failed to initialize database:\n%1\n\nPlease check your database file or contact support.")
                .arg(QString::fromUtf8(e.what())));
        return 1;
    }

    // Start traffic-statistics maintenance (startup downsample + background rollup).
    Stats::trafficStatsManager->Init();

    // Store Flags
    Configs::dataManager->settingsRepo->argv = arguments;
    if (Configs::dataManager->settingsRepo->argv.contains("-many")) Configs::dataManager->settingsRepo->flag_many = true;
    if (Configs::dataManager->settingsRepo->argv.contains("-tray")) Configs::dataManager->settingsRepo->flag_tray = true;
    if (Configs::dataManager->settingsRepo->argv.contains("-debug")) Configs::dataManager->settingsRepo->flag_debug = true;
    if (Configs::dataManager->settingsRepo->argv.contains("-flag_restart_tun_on")) Configs::dataManager->settingsRepo->flag_restart_tun_on = true;
    if (Configs::dataManager->settingsRepo->argv.contains("-flag_restart_dns_set")) Configs::dataManager->settingsRepo->flag_dns_set = true;
    Configs::dataManager->settingsRepo->flag_use_appdata = useAppdata;
    if(useAppdata && !appdataDir.isEmpty()) Configs::dataManager->settingsRepo->appdataDir = appdataDir;
#ifdef NKR_CPP_DEBUG
    Configs::dataManager->settingsRepo->flag_debug = true;
#endif

#ifdef Q_OS_LINUX
    QApplication::addLibraryPath(QApplication::applicationDirPath() + "/usr/plugins");
#endif

    // dispatchers
    DS_cores = new QThread;
    DS_cores->start();

    LogThread = new QThread;
    LogThread->start();

// icons
    QIcon::setFallbackSearchPaths(QStringList{
        ":/icon",
    });

    // icon for no theme
    if (QIcon::themeName().isEmpty()) {
        QIcon::setThemeName("breeze");
    }

#ifdef Q_OS_WIN
    if (Configs::dataManager->settingsRepo->windows_set_admin && !Configs::IsAdmin() && !Configs::dataManager->settingsRepo->disable_run_admin)
    {
        Configs::dataManager->settingsRepo->windows_set_admin = false; // so that if permission denied, we will run as user on the next run
        Configs::dataManager->settingsRepo->Save();
        WinCommander::runProcessElevated(QApplication::applicationFilePath(), {}, "", 1, false);
        QApplication::quit();
        return 0;
    }
#endif

    // dataManager->settingsRepo & Flags
    if (Configs::dataManager->settingsRepo->start_minimal) Configs::dataManager->settingsRepo->flag_tray = true;

    // Translate
    QGuiApplication::tr("QT_LAYOUT_DIRECTION");
    loadTranslate(Configs::dataManager->settingsRepo->language);

    // Check if another instance is running
#ifdef Q_OS_MACOS
    const QByteArray instanceIdentity = QByteArray("uid:") + QByteArray::number(getuid());
#else
    const QByteArray instanceIdentity = wd.absolutePath().toUtf8();
#endif
    QByteArray hashBytes = QCryptographicHash::hash(instanceIdentity, QCryptographicHash::Md5).toBase64(QByteArray::OmitTrailingEquals);
    hashBytes.replace('+', '0').replace('/', '1');
    auto serverName = LOCAL_SERVER_PREFIX + QString::fromUtf8(hashBytes);
    const auto restartProbeServerName = serverName + QStringLiteral("-restart-probe");
    qDebug() << "server name: " << serverName;
    if (waitForRestart) {
        for (int attempt = 0; attempt < 50; ++attempt) {
            QLocalSocket previousInstance;
            previousInstance.connectToServer(restartProbeServerName);
            if (!previousInstance.waitForConnected(100)) break;
            previousInstance.disconnectFromServer();
            QThread::msleep(50);
        }
    }
    QLocalSocket socket;
    socket.connectToServer(serverName);
    if (socket.waitForConnected(250))
    {
        qDebug() << "Another instance is running, let's wake it up and quit";
        // Hand off a deeplink (if any) so the primary instance handles it.
        if (!launchDeeplink.isEmpty()) {
            socket.write(launchDeeplink.toUtf8());
            socket.flush();
            socket.waitForBytesWritten(250);
        }
        socket.disconnectFromServer();
        return 0;
    }

    // QLocalServer
    QLocalServer server(qApp);
#ifdef Q_OS_MACOS
    server.setSocketOptions(QLocalServer::UserAccessOption);
#else
    server.setSocketOptions(QLocalServer::WorldAccessOption);
#endif
    if (!server.listen(serverName)) {
        qWarning() << "Failed to start QLocalServer! Error:" << server.errorString();
        return 1;
    }

    QLocalServer restartProbeServer(qApp);
    restartProbeServer.setSocketOptions(QLocalServer::UserAccessOption);
    QLocalServer::removeServer(restartProbeServerName);
    if (!restartProbeServer.listen(restartProbeServerName)) {
        qWarning() << "Failed to start restart probe server! Error:" << restartProbeServer.errorString();
    } else {
        QObject::connect(&restartProbeServer, &QLocalServer::newConnection, qApp, [&] {
            while (restartProbeServer.hasPendingConnections()) {
                auto *probe = restartProbeServer.nextPendingConnection();
                probe->disconnectFromServer();
                probe->deleteLater();
            }
        });
    }

    QObject::connect(&server, &QLocalServer::newConnection, qApp, [&] {
        auto s = server.nextPendingConnection();
        qDebug() << "Another instance tried to wake us up on " << serverName << s;
        // The waking instance may forward a throne:// deeplink as payload.
        auto readPayload = [s] {
            if (s->bytesAvailable() <= 0) return;
            Deeplink_Submit(QString::fromUtf8(s->readAll()).trimmed());
        };
        QObject::connect(s, &QLocalSocket::readyRead, s, readPayload);
        QObject::connect(s, &QLocalSocket::disconnected, s, &QLocalSocket::deleteLater);
        readPayload(); // in case the payload already arrived
        // raise main window
        MW_dialog_message(MwMessage::Raise, {});
    });
    QObject::connect(qApp, &QApplication::aboutToQuit, [&]
    {
        restartProbeServer.close();
        QLocalServer::removeServer(restartProbeServerName);
        server.close();
        QLocalServer::removeServer(serverName);
    });

#ifdef Q_OS_LINUX
    signal(SIGTERM, signal_handler);
    signal(SIGINT, signal_handler);
#endif

#ifdef Q_OS_WIN
    auto eventFilter = new PowerOffTaskkillFilter(signal_handler);
    a.installNativeEventFilter(eventFilter);
#endif

#ifdef Q_OS_MACOS
    QObject::connect(qApp, &QGuiApplication::commitDataRequest, [&](QSessionManager &manager)
    {
        Q_UNUSED(manager);
        signal_handler(0);
    });
#endif

    API::defaultClient = new API::Client();

    UI_InitMainWindow();

#ifdef Q_OS_LINUX
    QTimer terminationTimer;
    terminationTimer.setInterval(100);
    QObject::connect(&terminationTimer, &QTimer::timeout, qApp, [] {
        if (!terminationRequested) return;
        terminationRequested = 0;
        GetMainWindow()->prepare_exit();
        qApp->quit();
    });
    terminationTimer.start();
#endif

    // Deliver a deeplink passed on the command line (cold start), and replay any that
    // arrived during startup (e.g. a macOS FileOpen event before the window existed).
    if (!launchDeeplink.isEmpty()) Deeplink_Submit(launchDeeplink);
    Deeplink_FlushPending();

    return QApplication::exec();
}
