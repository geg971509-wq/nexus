// Guards ThemeManager::isDarkTheme, which decides the log viewer's highlight mode.
// The custom-theme answers must stay pinned to each palette's own brightness.
#include "include/ui/setting/ThemeManager.hpp"

#include <QApplication>
#include <QStyleHints>

#include <cstdlib>

// ThemeManager reads its stylesheets through this; the test never needs them.
QString ReadFileText(const QString &) { return {}; }

static bool failed = false;

static void expect(const QString &theme, bool wantDark) {
    const bool got = themeManager->isDarkTheme(theme);
    if (got == wantDark) return;
    qWarning() << "isDarkTheme(" << theme << ") =" << got << "expected" << wantDark;
    failed = true;
}

int main(int argc, char **argv) {
    qputenv("QT_QPA_PLATFORM", "offscreen");
    QApplication app(argc, argv);

    // Stylesheet themes carry their own palette, so the answer is fixed.
    expect("flatgray", false);
    expect("lightblue", false);
    expect("softpink", false);
    expect("blacksoft", true);
    expect("qdarkstyle", true);

    // The combo box offers mixed case; lookup must not care.
    expect("FlatGray", false);
    expect("LightBlue", false);
    expect("SoftPink", false);
    expect("BlackSoft", true);
    expect("QDarkStyle", true);

    // windowsvista is light-only regardless of the OS colour scheme.
    expect("windowsvista", false);

    // Everything else defers to the system preference.
    const bool systemDark = qApp->styleHints()->colorScheme() == Qt::ColorScheme::Dark;
    expect("Fusion", systemDark);
    expect("system", systemDark);

    return failed ? EXIT_FAILURE : EXIT_SUCCESS;
}
