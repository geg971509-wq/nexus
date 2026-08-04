#include "include/global/Utils.hpp"

#include "3rdparty/QThreadCreateThread.hpp"

#include <memory>
#include <random>
#include <vector>

#include <QApplication>
#include <QCoreApplication>
#include <QEventLoop>
#include <QUrlQuery>
#include <QTcpServer>
#include <QTimer>
#include <QMessageBox>
#include <QFile>
#include <QJsonObject>
#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QDateTime>
#include <QLocale>
#include <QMutex>
#include <QPointer>
#include <QSet>
#include <QThread>
#include <QCheckBox>
#include <QLayout>
#include <QVBoxLayout>
#include <QPlainTextEdit>
#include <QDialogButtonBox>
#include <QDialog>

#ifdef Q_OS_WIN
#include "include/sys/windows/guihelper.h"
#endif

namespace {
    QMutex backgroundThreadsMutex;
    QSet<QThread*> backgroundThreads;
}
#ifdef Q_OS_MACOS
// TransformProcessType lives in HIServices/Processes.h.
// Umbrella ApplicationServices also pulls QD/ColorSync deprecations into this TU.
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
#pragma clang diagnostic ignored "-Wdeprecated-anon-enum-enum-conversion"
#pragma clang diagnostic ignored "-Wnullability-completeness"
#include <ApplicationServices/ApplicationServices.h>
#pragma clang diagnostic pop
#endif

QStringList SplitLines(const QString &_string) {
    return _string.split(QRegularExpression("[\r\n]"), Qt::SplitBehaviorFlags::SkipEmptyParts);
}

QStringList SplitLinesSkipSharp(const QString &_string, int maxLine) {
    auto lines = SplitLines(_string);
    QStringList newLines;
    int i = 0;
    for (const auto &line: lines) {
        if (line.trimmed().startsWith("#")) continue;
        newLines << line;
        if (maxLine > 0 && ++i >= maxLine) break;
    }
    return newLines;
}

QByteArray DecodeB64IfValid(const QString &input, QByteArray::Base64Options options) {
    QByteArray::Base64Options newOptions = options | QByteArray::Base64Option::AbortOnBase64DecodingErrors;
    auto result = QByteArray::fromBase64Encoding(input.toUtf8(), newOptions);
    if (result) {
        return result.decoded;
    }
    return {};
}

QStringList SplitAndTrim(const QString& raw, const QString& separator, bool keepEmpty) {
    QStringList result;
    auto spl = raw.split(separator);
    for (const auto& str : spl) {
        auto trimmed = str.trimmed();
        if (!keepEmpty && trimmed.isEmpty()) continue;
        result << trimmed;
    }
    return result;
}

QString QStringList2Command(const QStringList &list) {
    QStringList new_list;
    for (auto str: list) {
        auto q = "\"" + str.replace("\"", "\\\"") + "\"";
        new_list << q;
    }
    return new_list.join(" ");
}

QString GetQueryValue(const QUrlQuery &q, const QString &key, const QString &def) {
    auto a = q.queryItemValue(key);
    if (a.isEmpty()) {
        return def;
    }
    return a;
}

QString GetRandomString(int randomStringLength) {
    std::random_device rd;
    std::mt19937 mt(rd());

    const QString possibleCharacters("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789");

    std::uniform_int_distribution<int> dist(0, possibleCharacters.length() - 1);

    QString randomString;
    for (int i = 0; i < randomStringLength; ++i) {
        QChar nextChar = possibleCharacters.at(dist(mt));
        randomString.append(nextChar);
    }
    return randomString;
}

quint64 GetRandomUint64() {
    std::random_device rd;
    std::mt19937 mt(rd());
    std::uniform_int_distribution<quint64> dist;
    return dist(mt);
}

QString GenRandomLoopback() {
#ifdef Q_OS_MACOS
    return "127.0.0.1";
#else
    std::random_device rd;
    std::mt19937 mt(rd());
    std::uniform_int_distribution<int> octet(1, 254);
    return QString("127.%1.%2.%3").arg(octet(mt)).arg(octet(mt)).arg(octet(mt));
#endif
}

// QString >> QJson
QJsonObject QString2QJsonObject(const QString &jsonString) {
    QJsonDocument jsonDocument = QJsonDocument::fromJson(jsonString.toUtf8());
    QJsonObject jsonObject = jsonDocument.object();
    return jsonObject;
}

// QJson >> QString
QString QJsonObject2QString(const QJsonObject &jsonObject, bool compact) {
    return QJsonDocument(jsonObject).toJson(compact ? QJsonDocument::Compact : QJsonDocument::Indented);
}

QJsonArray QListStr2QJsonArray(const QList<QString> &list) {
    QVariantList list2;
    bool isEmpty = true;
    for (auto &item: list) {
        if (item.trimmed().isEmpty()) continue;
        list2.append(item);
        isEmpty = false;
    }

    if (isEmpty) return {};
    else return QJsonArray::fromVariantList(list2);
}

QJsonArray QListInt2QJsonArray(const QList<int> &list) {
    QVariantList list2;
    for (auto &item: list)
        list2.append(item);
    return QJsonArray::fromVariantList(list2);
}

QList<int> QJsonArray2QListInt(const QJsonArray &arr) {
    QList<int> list2;
    for (auto item: arr)
        list2.append(item.toInt());
    return list2;
}

QList<QString> QJsonArray2QListString(const QJsonArray &arr) {
    QList<QString> list2;
    for (auto item: arr)
        list2.append(item.toString());
    return list2;
}

QJsonArray QString2QJsonArray(const QString& str) {
    auto doc = QJsonDocument::fromJson(str.toUtf8());
    if (doc.isArray()) {
        return doc.array();
    }
    return {};
}

QJsonObject QMapString2QJsonObject(const QMap<QString,QString> &mp) {
    QJsonObject res;
    for (const auto &key: mp.keys()) {
        res.insert(key, mp[key]);
    }

    return res;
}

QList<QString> QListInt2QListString(const QList<int> &list) {
    QList<QString> resp;
    for (int item : list) resp << Int2String(item);
    return resp;
}

QList<int> QStringList2QListInt(const QList<QString> &list) {
    QList<int> resp;
    for (auto item: list) resp.append(item.toInt());
    return resp;
}

QByteArray ReadFile(const QString &path) {
    QFile file(path);
    if (!file.open(QFile::ReadOnly)) return {};
    return file.readAll();
}

QString ReadFileText(const QString &path) {
    QFile file(path);
    if (!file.open(QFile::ReadOnly | QFile::Text)) return {};
    QTextStream stream(&file);
    return stream.readAll();
}

int MkPort() {
    QTcpServer s;
    s.listen();
    auto port = s.serverPort();
    s.close();
    return port;
}

QList<int> MkManyPorts(int num) {
    QList<int> res;
    std::vector<std::unique_ptr<QTcpServer>> servers;
    res.reserve(num);
    servers.reserve(static_cast<size_t>(num));
    for (int i = 0; i < num; i++) {
        auto server = std::make_unique<QTcpServer>();
        server->listen();
        res.append(server->serverPort());
        servers.push_back(std::move(server));
    }
    return res;
}

QString ReadableSize(const qint64 &size) {
    double sizeAsDouble = size;
    static QStringList measures;
    if (measures.isEmpty())
        measures << "B"
                 << "KiB"
                 << "MiB"
                 << "GiB"
                 << "TiB"
                 << "PiB"
                 << "EiB"
                 << "ZiB"
                 << "YiB";
    QStringListIterator it(measures);
    QString measure(it.next());
    while (sizeAsDouble >= 1024.0 && it.hasNext()) {
        measure = it.next();
        sizeAsDouble /= 1024.0;
    }
    return QString::fromLatin1("%1 %2").arg(sizeAsDouble, 0, 'f', 2).arg(measure);
}

bool IsIpAddress(const QString &str) {
    auto address = QHostAddress(str);
    if (address.protocol() == QAbstractSocket::IPv4Protocol || address.protocol() == QAbstractSocket::IPv6Protocol)
        return true;
    return false;
}

bool IsIpAddressV4(const QString &str) {
    auto address = QHostAddress(str);
    if (address.protocol() == QAbstractSocket::IPv4Protocol)
        return true;
    return false;
}

bool IsIpAddressV6(const QString &str) {
    auto address = QHostAddress(str);
    if (address.protocol() == QAbstractSocket::IPv6Protocol)
        return true;
    return false;
}

QString DisplayTime(long long time, int formatType) {
    QDateTime t;
    t.setMSecsSinceEpoch(time * 1000);
    return QLocale().toString(t, QLocale::FormatType(formatType));
}

QWidget *GetMessageBoxParent() {
    // Prefer the long-lived main window. Temporary dialogs (e.g. DialogManageRoutes)
    // are often deleteLater()'d while a nested QMessageBox::exec() processes events;
    // parenting the box to that dialog then free-not-allocated crashes in deleteChildren.
    if (mainwindow != nullptr) return mainwindow;
    auto *activeWindow = QApplication::activeWindow();
    return activeWindow;
}

int MessageBoxWarning(const QString &title, const QString &text) {
    return QMessageBox::warning(GetMessageBoxParent(), title, text);
}

int MessageBoxInfo(const QString &title, const QString &text) {
    return QMessageBox::information(GetMessageBoxParent(), title, text);
}

void MessageBoxScrollable(const QString &title, const QString &text) {
    QDialog dialog(GetMessageBoxParent());
    dialog.setWindowTitle(title);
    auto *layout = new QVBoxLayout(&dialog);
    auto *view = new QPlainTextEdit(&dialog);
    view->setPlainText(text);
    view->setReadOnly(true);
    layout->addWidget(view);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok, &dialog);
    QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    layout->addWidget(buttons);
    dialog.resize(480, 420);
    dialog.exec();
}

int MessageBoxCheck(const QString &title, const QString &text, const QString &checkBoxText, bool &isChecked) {
    QMessageBox msgBox(GetMessageBoxParent());
    msgBox.setWindowTitle(title);
    msgBox.setText(text);
    msgBox.setIcon(QMessageBox::Question);
    msgBox.setStandardButtons(QMessageBox::Ok | QMessageBox::Cancel);
    msgBox.setDefaultButton(QMessageBox::Ok);

    QCheckBox *checkBox = new QCheckBox(checkBoxText);
    checkBox->setChecked(isChecked);

    dynamic_cast< QGridLayout *>(msgBox.layout())->addWidget(checkBox, 1, 2);

    int result = msgBox.exec();

    isChecked = checkBox->isChecked();

    return result;
}

void ActivateWindow(QWidget *w) {
    w->setWindowState(w->windowState() & ~Qt::WindowMinimized);
    w->setVisible(true);
#ifdef Q_OS_WIN
    Windows_QWidget_SetForegroundWindow(w);
#elif defined(Q_OS_MACOS)
    ProcessSerialNumber psn = { 0, kCurrentProcess };
    TransformProcessType(&psn, kProcessTransformToForegroundApplication);
#endif
    w->raise();
    w->activateWindow();
}

void HideWindow(QWidget *w) {
    w->hide();
#ifdef Q_OS_MACOS
    ProcessSerialNumber psn = { 0, kCurrentProcess };
    TransformProcessType(&psn, kProcessTransformToUIElementApplication);
#endif
}

void runOnUiThread(const std::function<void()> &callback, bool wait) {
    // Prefer mainwindow affinity; fall back to the QApp thread before UI exists
    // (DB init / NotifyError) so we never null-deref mainwindow.
    QThread *thread = nullptr;
    if (mainwindow) {
        thread = mainwindow->thread();
    } else if (auto *app = QCoreApplication::instance()) {
        thread = app->thread();
    }
    if (!thread) {
        // No event loop yet: run inline so callers still make progress.
        callback();
        return;
    }
    if (thread == QThread::currentThread()) {
        callback();
        return;
    }
    auto *timer = new QTimer();
    timer->moveToThread(thread);
    timer->setSingleShot(true);

    QEventLoop loop;
    QObject::connect(timer, &QTimer::timeout, [=, &loop]() {
        // UI / app thread
        try {
            callback();
        } catch (...) {
            // A throwing callback must not skip the cleanup below: without the
            // deleteLater/quit posts the timer leaks (and for runOnNewThread
            // the QThread event loop spins forever, hanging
            // waitForBackgroundThreads()).
            qWarning() << "runOnUiThread: callback threw an exception; swallowed";
        }
        timer->deleteLater();

        if (wait)
        {
            QMetaObject::invokeMethod(&loop, "quit", Qt::QueuedConnection);
        }
    });
    QMetaObject::invokeMethod(timer, "start", Qt::QueuedConnection, Q_ARG(int, 0));

    if (wait && QThread::currentThread() != thread) {
        loop.exec();
    }
}

static QString g_pendingDeeplink;

QString Deeplink_ExtractFromArgs(const QStringList &args) {
    for (const auto &arg : args) {
        if (arg.startsWith("throne://")) return arg;
    }
    return {};
}

void Deeplink_Submit(const QString &url) {
    if (url.isEmpty() || !url.startsWith("throne://")) return;
    if (MW_handle_deeplink) {
        MW_handle_deeplink(url);
    } else {
        g_pendingDeeplink = url; // main window not up yet; replayed by Deeplink_FlushPending
    }
}

void Deeplink_FlushPending() {
    if (g_pendingDeeplink.isEmpty() || !MW_handle_deeplink) return;
    const QString url = g_pendingDeeplink;
    g_pendingDeeplink.clear();
    MW_handle_deeplink(url);
}

void runOnNewThread(const std::function<void()> &callback, bool wait) {
    auto *timer = new QTimer();
    auto thread = new QThread();
    timer->moveToThread(thread);
    timer->setSingleShot(true);

    {
        QMutexLocker lock(&backgroundThreadsMutex);
        backgroundThreads.insert(thread);
    }
    QObject::connect(thread, &QThread::finished, thread, [thread] {
        QMutexLocker lock(&backgroundThreadsMutex);
        backgroundThreads.remove(thread);
    }, Qt::DirectConnection);
    thread->start();
    QObject::connect(thread, &QThread::finished, thread, &QObject::deleteLater);

    QEventLoop loop;
    QObject::connect(timer, &QTimer::timeout, [=, &loop]() {
        try {
            callback();
        } catch (...) {
            // Keep the cleanup below unconditional: a throwing callback must
            // not leak the timer or leave this thread's event loop spinning
            // (waitForBackgroundThreads() would hang in ~MainWindow).
            qWarning() << "runOnNewThread: callback threw an exception; swallowed";
        }
        timer->deleteLater();
        QMetaObject::invokeMethod(thread, "quit", Qt::QueuedConnection);

        if (wait)
        {
            QMetaObject::invokeMethod(&loop, "quit", Qt::QueuedConnection);
        }
    });
    QMetaObject::invokeMethod(timer, "start", Qt::QueuedConnection, Q_ARG(int, 0));

    if (wait && QThread::currentThread() != thread) {
        loop.exec();
    }
}

void waitForBackgroundThreads() {
    while (true) {
        QList<QPointer<QThread>> threads;
        {
            QMutexLocker lock(&backgroundThreadsMutex);
            if (backgroundThreads.isEmpty()) return;
            threads.reserve(backgroundThreads.size());
            for (auto *thread : backgroundThreads) threads.append(QPointer<QThread>(thread));
        }

        for (const auto& thread : threads) {
            while (thread && thread->isRunning() && !thread->wait(20)) {
                QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
            }
        }
        QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
    }
}

void runOnThread(const std::function<void()> &callback, QObject *parent, bool wait) {
    auto *timer = new QTimer();
    auto thread = dynamic_cast<QThread *>(parent);
    if (thread == nullptr) {
        timer->moveToThread(parent->thread());
        thread = parent->thread();
    } else {
        timer->moveToThread(thread);
    }
    timer->setSingleShot(true);

    QEventLoop loop;
    QObject::connect(timer, &QTimer::timeout, [=, &loop]() {
        try {
            callback();
        } catch (...) {
            // Keep the cleanup below unconditional (see runOnUiThread).
            qWarning() << "runOnThread: callback threw an exception; swallowed";
        }
        timer->deleteLater();

        if (wait)
        {
            QMetaObject::invokeMethod(&loop, "quit", Qt::QueuedConnection);
        }
    });
    QMetaObject::invokeMethod(timer, "start", Qt::QueuedConnection, Q_ARG(int, 0));

    if (wait && QThread::currentThread() != thread) {
        loop.exec();
    }
}

void setTimeout(const std::function<void()> &callback, QObject *obj, int timeout) {
    QTimer::singleShot(timeout, obj, std::move(callback));
}
