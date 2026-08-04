#include "include/sys/UrlScheme.hpp"

#include <QApplication>
#include <QDir>
#include <QSettings>

// Per-user scheme registration under HKCU\Software\Classes — needs no admin and
// takes precedence over any system-wide handler. In QSettings NativeFormat the
// value name "Default" maps to a registry key's unnamed (Default) value, and
// '/' separates subkeys.

static const QString kSchemeRoot = "HKEY_CURRENT_USER\\Software\\Classes\\throne";

QString UrlScheme_DesiredState() {
    const QString exe = QDir::toNativeSeparators(QApplication::applicationFilePath());
    return "\"" + exe + "\" \"%1\"";
}

void UrlScheme_Apply() {
    QSettings reg(kSchemeRoot, QSettings::NativeFormat);
    reg.setValue("Default", "URL:Throne Protocol");
    reg.setValue("URL Protocol", "");
    reg.setValue("shell/open/command/Default", UrlScheme_DesiredState());
}
