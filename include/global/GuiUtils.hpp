#pragma once

#include <QRegularExpression>
#include <QRegularExpressionValidator>
#include <QString>

#include "include/configs/common/utils.h"
#include "include/database/DatabaseManager.h"

// Utils

#define QRegExpValidator_Number new QRegularExpressionValidator(QRegularExpression("^[0-9]+$"), this)

// Save&Load

// Widget <-> Configs::SettingsRepo field load/save helpers (replacing the old
// identifier-glued D_LOAD_*/D_SAVE_* macros). Each takes the widget plus a
// member pointer to the SettingsRepo field, e.g.
//     dLoadText(ui->inbound_address, &Configs::SettingsRepo::inbound_address);

namespace GuiUtils {
    inline Configs::SettingsRepo *settingsRepo() {
        return Configs::dataManager->settingsRepo.get();
    }

    // QLineEdit-style widget <-> QString field.
    template <typename W>
    void dLoadText(W *w, QString Configs::SettingsRepo::*m) {
        w->setText(settingsRepo()->*m);
    }

    template <typename W>
    void dSaveText(W *w, QString Configs::SettingsRepo::*m) {
        settingsRepo()->*m = w->text();
    }

    // QLineEdit-style widget <-> int field. Load pins a digits-only validator.
    template <typename W>
    void dLoadInt(W *w, int Configs::SettingsRepo::*m) {
        w->setText(QString::number(settingsRepo()->*m));
        w->setValidator(new QRegularExpressionValidator(QRegularExpression("^[0-9]+$"), w));
    }

    // A non-numeric edit keeps the stored value instead of collapsing it to 0.
    template <typename W>
    void dSaveInt(W *w, int Configs::SettingsRepo::*m) {
        auto *repo = settingsRepo();
        repo->*m = Configs::parseIntOr(w->text(), repo->*m);
    }

    // QComboBox <-> QString field.
    template <typename W>
    void dLoadComboText(W *w, QString Configs::SettingsRepo::*m) {
        w->setCurrentText(settingsRepo()->*m);
    }

    template <typename W>
    void dSaveComboText(W *w, QString Configs::SettingsRepo::*m) {
        settingsRepo()->*m = w->currentText();
    }

    // Checkable widget <-> bool field.
    template <typename W>
    void dLoadBool(W *w, bool Configs::SettingsRepo::*m) {
        w->setChecked(settingsRepo()->*m);
    }

    template <typename W>
    void dSaveBool(W *w, bool Configs::SettingsRepo::*m) {
        settingsRepo()->*m = w->isChecked();
    }

    // Signed int field edited as a magnitude line edit plus an enable checkbox:
    // positive value = enabled, negative = disabled, magnitude = the number.
    template <typename W, typename E>
    void dLoadIntEnable(W *w, E *e, int Configs::SettingsRepo::*m) {
        const int v = settingsRepo()->*m;
        e->setChecked(v > 0);
        w->setText(QString::number(v > 0 ? v : -v));
        w->setValidator(new QRegularExpressionValidator(QRegularExpression("^[0-9]+$"), w));
    }

    // A non-numeric edit keeps the stored magnitude instead of collapsing to 0.
    template <typename W, typename E>
    void dSaveIntEnable(W *w, E *e, int Configs::SettingsRepo::*m) {
        auto *repo = settingsRepo();
        const int old = repo->*m;
        const int v = Configs::parseIntOr(w->text(), old > 0 ? old : -old);
        repo->*m = e->isChecked() ? v : -v;
    }
} // namespace GuiUtils

using GuiUtils::dLoadText;
using GuiUtils::dSaveText;
using GuiUtils::dLoadInt;
using GuiUtils::dSaveInt;
using GuiUtils::dLoadComboText;
using GuiUtils::dSaveComboText;
using GuiUtils::dLoadBool;
using GuiUtils::dSaveBool;
using GuiUtils::dLoadIntEnable;
using GuiUtils::dSaveIntEnable;

#define C_EDIT_JSON_ALLOW_EMPTY(a)                                    \
    auto editor = new JsonEditor(QString2QJsonObject(CACHE.a), this); \
    auto result = editor->OpenEditor();                               \
    CACHE.a = QJsonObject2QString(result, true);                      \
    if (result.isEmpty()) CACHE.a = "";                               \
    editor->deleteLater();

//

#define ADD_ASTERISK(parent)                                         \
    for (auto label: parent->findChildren<QLabel *>()) {             \
        auto text = label->text();                                   \
        if (!label->toolTip().isEmpty() && !text.endsWith("*")) {    \
            label->setText(text + "*");                              \
        }                                                            \
    }                                                                \
    for (auto checkBox: parent->findChildren<QCheckBox *>()) {       \
        auto text = checkBox->text();                                \
        if (!checkBox->toolTip().isEmpty() && !text.endsWith("*")) { \
            checkBox->setText(text + "*");                           \
        }                                                            \
    }
