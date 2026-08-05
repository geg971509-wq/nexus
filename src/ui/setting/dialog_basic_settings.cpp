#include "include/ui/setting/dialog_basic_settings.h"

#include "3rdparty/qv2ray/v2/ui/widgets/editors/w_JsonEditor.hpp"
#include "include/ui/setting/ThemeManager.hpp"
#include "include/ui/setting/Icon.hpp"
#include "include/ui/setting/BackupArchive.h"
#include "include/ui/setting/BackupRestore.h"
#include "include/global/GuiUtils.hpp"
#include "include/global/Configs.hpp"
#include "include/global/HTTPRequestHelper.hpp"
#include "include/global/DeviceDetailsHelper.hpp"
#include "include/stats/traffic/TrafficLooper.hpp"

#include <QStyleFactory>
#include <QFileDialog>
#include <QInputDialog>
#include <QMessageBox>
#include <QTimer>
#include <QBrush>
#include <QRegularExpression>
#include <QTextBlock>
#include <QTextCursor>
#include <qfontdatabase.h>
#include <QDateTime>
#include <QFileInfo>
#include <QSaveFile>
#include <QTemporaryDir>
#include <QJsonObject>
#include <QSysInfo>
#include <QDir>
#include <QStandardPaths>
#include <QCheckBox>
#include <QScreen>
#include <QVBoxLayout>
#include <QDialogButtonBox>
#include <QLabel>
#include <QPushButton>

#include <tuple>

#include "include/ui/mainwindow.h"

DialogBasicSettings::DialogBasicSettings(QWidget *parent)
    : QDialog(parent), ui(new Ui::DialogBasicSettings) {
    ui->setupUi(this);
    ADD_ASTERISK(this);

    // Common
    ui->log_level->addItems(QString("trace debug info warn error fatal panic").split(" "));
    ui->xray_loglevel->addItems(Configs::Xray::XrayLogLevels);
    ui->mux_protocol->addItems({"h2mux", "smux", "yamux"});
    ui->fragment_implementation->addItems({"built-in", "custom"});
    ui->disable_stats->setChecked(Configs::dataManager->settingsRepo->disable_traffic_stats);
    ui->proxy_scheme->setCurrentText(Configs::dataManager->settingsRepo->proxy_scheme);

    dLoadText(ui->inbound_address, &Configs::SettingsRepo::inbound_address);
    CACHE.custom_inbound = Configs::dataManager->settingsRepo->custom_inbound;
    dLoadInt(ui->inbound_socks_port, &Configs::SettingsRepo::inbound_socks_port);
    ui->random_listen_port->setChecked(Configs::dataManager->settingsRepo->random_inbound_port);
    dLoadInt(ui->test_concurrent, &Configs::SettingsRepo::test_concurrent);
    dLoadText(ui->test_latency_url, &Configs::SettingsRepo::test_latency_url);
    dLoadBool(ui->disable_tray, &Configs::SettingsRepo::disable_tray);
    ui->reset_proxy_on_disable_sp->setChecked(Configs::dataManager->settingsRepo->reset_proxy_on_disable_sp);
    ui->url_timeout->setText(Int2String(Configs::dataManager->settingsRepo->url_test_timeout_ms));
    ui->speedtest_mode->setCurrentIndex(Configs::dataManager->settingsRepo->speed_test_mode);
    ui->test_timeout->setText(Int2String(Configs::dataManager->settingsRepo->speed_test_timeout_ms));
    ui->simple_down_url->setText(Configs::dataManager->settingsRepo->simple_dl_url);
    ui->allow_beta->setChecked(Configs::dataManager->settingsRepo->allow_beta_update);
    ui->disable_mixed_inbound->setChecked(Configs::dataManager->settingsRepo->disable_mixed_inbound);
    dLoadBool(ui->inbound_auth, &Configs::SettingsRepo::inbound_auth);
    dLoadText(ui->inbound_user, &Configs::SettingsRepo::inbound_user);
    dLoadText(ui->inbound_pass, &Configs::SettingsRepo::inbound_pass);

    connect(ui->custom_inbound_edit, &QPushButton::clicked, this, [=,this] {
        C_EDIT_JSON_ALLOW_EMPTY(custom_inbound)
    });
    connect(ui->disable_tray, &QCheckBox::checkStateChanged, this, [this](Qt::CheckState) {
        CACHE.updateDisableTray = true;
    });
    connect(ui->random_listen_port, &QCheckBox::checkStateChanged, this, [this](Qt::CheckState state) {
        ui->inbound_socks_port->setDisabled(state == Qt::Checked);
    });

#ifndef Q_OS_WIN
    ui->proxy_scheme_l->hide();
    ui->proxy_scheme->hide();
    ui->windows_no_admin->hide();
#endif

    // Logging
    ui->max_log_line->setText(QString::number(Configs::dataManager->settingsRepo->max_log_line));
    dLoadBool(ui->log_auto_scroll, &Configs::SettingsRepo::log_auto_scroll);
    ui->log_level->setCurrentText(Configs::dataManager->settingsRepo->log_level);
    ui->xray_loglevel->setCurrentText(Configs::dataManager->settingsRepo->xray_log_level);
    ui->enable_log_include->setChecked(Configs::dataManager->settingsRepo->log_enable_include);
    ui->enable_log_exclude->setChecked(Configs::dataManager->settingsRepo->log_enable_exclude);
    ui->log_include_keyword->setText(Configs::dataManager->settingsRepo->log_include_keyword.join("\n"));
    ui->log_exclude_keyword->setText(Configs::dataManager->settingsRepo->log_exclude_keyword.join("\n"));
    ui->log_include_regex->setText(Configs::dataManager->settingsRepo->log_include_regex.join("\n"));
    ui->log_exclude_regex->setText(Configs::dataManager->settingsRepo->log_exclude_regex.join("\n"));
    applyRegexHighlighting();

    connect(ui->log_include_regex, &QTextEdit::textChanged, this, [this] { applyRegexHighlighting(); });
    connect(ui->log_exclude_regex, &QTextEdit::textChanged, this, [this] { applyRegexHighlighting(); });

    // Style
    ui->connection_statistics->setChecked(Configs::dataManager->settingsRepo->enable_stats);
    ui->disable_traffic_aggregation->setChecked(Configs::dataManager->settingsRepo->disable_traffic_aggregation);
    ui->show_sys_dns->setChecked(Configs::dataManager->settingsRepo->show_system_dns);
    connect(ui->show_sys_dns, &QCheckBox::checkStateChanged, this, [this](Qt::CheckState) {
        CACHE.updateSystemDns = true;
    });
#ifndef Q_OS_WIN
    ui->show_sys_dns->hide();
#endif
    //
    dLoadBool(ui->start_minimal, &Configs::SettingsRepo::start_minimal);
    ui->skip_delete_confirm->setChecked(Configs::dataManager->settingsRepo->skip_delete_confirmation);
    dLoadBool(ui->show_config_security, &Configs::SettingsRepo::show_config_security);
    //
    ui->language->setCurrentIndex(Configs::dataManager->settingsRepo->language);
    connect(ui->font, &QComboBox::currentTextChanged, this, [=,this](const QString &fontName) {
        auto font = qApp->font();
        font.setFamily(fontName);
        qApp->setFont(font);
        Configs::dataManager->settingsRepo->font = fontName;
        Configs::dataManager->settingsRepo->Save();
        adjustSize();
    });
    for (int i=7;i<=26;i++) {
        ui->font_size->addItem(Int2String(i));
    }
    ui->font_size->setCurrentText(Int2String(qApp->font().pointSize()));
    connect(ui->font_size, &QComboBox::currentTextChanged, this, [=,this](const QString &sizeStr) {
        auto font = qApp->font();
        font.setPointSize(sizeStr.toInt());
        qApp->setFont(font);
        Configs::dataManager->settingsRepo->font_size = sizeStr.toInt();
        Configs::dataManager->settingsRepo->Save();
        adjustSize();
    });
    //
    ui->theme->addItems(QStyleFactory::keys());
    ui->theme->addItem("QDarkStyle");
    // feiyangqingyun custom stylesheet themes (ported from upstream nekoray)
    ui->theme->addItems({"FlatGray", "LightBlue", "SoftPink", "BlackSoft"});
    ui->enable_custom_icon->setChecked(Configs::dataManager->settingsRepo->use_custom_icons);
    connect(ui->select_custom_icon, &QPushButton::clicked, this, [=, this] {
        auto n = QMessageBox::information(
            this,
            tr("Custom Icon Manual"),
            tr("To choose custom icons, you need to choose png images with an equal width and height (eg 512*512). Their names should be of\n"
               "(Dns.png, Off.png, Proxy.png, Proxy-Dns.png, Nexus.png, Tun.png) So that each will be used in the appropriate state of the app.\n"
               "You can provide a subset of the said images and only the corresponding states will be using them.\n"
               "It is suggested that each image's size be less than 100KB."),
            QMessageBox::Open | QMessageBox::Cancel);
        if (n == QMessageBox::Open) {
            auto fileNames = QFileDialog::getOpenFileNames(this,
                tr("Select png icons"), QDir::homePath(), tr("Image Files (*.png)"));
            // process files
            QString errors;
            for (const auto& fileName : fileNames) {
                CACHE.updateTrayIcon = true;
                QFileInfo fileInfo(fileName);
                if (auto pixMap = QPixmap(fileName); pixMap.isNull()) {
                    errors += tr("Failed to load %1").arg(fileName) + "\n";
                } else if (pixMap.width() != pixMap.height()) {
                    errors += tr("Image does not have equal width and height: %1").arg(fileName) + "\n";
                } else if (!Configs::Information::iconNames.contains(fileInfo.fileName())) {
                    errors += tr("Icon name is not valid: %1").arg(fileInfo.fileName()) + "\n";
                } else {
                    QFile::remove(QDir("icons").filePath(fileInfo.fileName()));
                    if (!QFile::copy(fileName, QDir("icons").filePath(fileInfo.fileName()))) {
                        errors += tr("Failed to copy %1").arg(fileName) + "\n";
                    }
                }
            }
            if (!errors.isEmpty()) {
                QMessageBox::warning(this, tr("Select custom image error"), errors);
            }
        }
    });
    //
    bool ok;
    auto themeId = Configs::dataManager->settingsRepo->theme.toInt(&ok);
    if (ok) {
        ui->theme->setCurrentIndex(themeId);
    } else {
        ui->theme->setCurrentText(Configs::dataManager->settingsRepo->theme);
    }
    //
    connect(ui->theme, &QComboBox::currentIndexChanged, this, [=,this](int index) {
        themeManager->ApplyTheme(ui->theme->currentText());
        Configs::dataManager->settingsRepo->theme = ui->theme->currentText();
        Configs::dataManager->settingsRepo->Save();
    });

    // Subscription

    ui->user_agent->setText(Configs::dataManager->settingsRepo->user_agent);
    ui->user_agent->setPlaceholderText(Configs::dataManager->settingsRepo->GetUserAgent(true));
    dLoadBool(ui->net_use_proxy, &Configs::SettingsRepo::net_use_proxy);
    dLoadBool(ui->sub_clear, &Configs::SettingsRepo::sub_clear);
    dLoadBool(ui->sub_show_change_popup, &Configs::SettingsRepo::sub_show_change_popup);
    dLoadBool(ui->net_insecure, &Configs::SettingsRepo::net_insecure);
    dLoadBool(ui->sub_send_hwid, &Configs::SettingsRepo::sub_send_hwid);
    dLoadBool(ui->allow_stopping_active_profile, &Configs::SettingsRepo::allow_stopping_active_profile);
    dLoadText(ui->sub_custom_hwid_params, &Configs::SettingsRepo::sub_custom_hwid_params);
    dLoadIntEnable(ui->sub_auto_update, ui->sub_auto_update_enable, &Configs::SettingsRepo::sub_auto_update);
    dLoadIntEnable(ui->route_auto_update, ui->route_auto_update_enable, &Configs::SettingsRepo::route_auto_update);
    retranslateDynamicUi();

    // Mux
    dLoadInt(ui->mux_concurrency, &Configs::SettingsRepo::mux_concurrency);
    dLoadComboText(ui->mux_protocol, &Configs::SettingsRepo::mux_protocol);
    dLoadBool(ui->mux_padding, &Configs::SettingsRepo::mux_padding);
    dLoadBool(ui->mux_default_on, &Configs::SettingsRepo::mux_default_on);
    dLoadComboText(ui->fragment_implementation, &Configs::SettingsRepo::fragment_implementation);
    dLoadBool(ui->fragment_default_on, &Configs::SettingsRepo::fragment_default_on);
    dLoadBool(ui->tls_tricks_default_on, &Configs::SettingsRepo::tls_tricks_default_on);
    dLoadText(ui->fragment_size, &Configs::SettingsRepo::fragment_size);
    dLoadText(ui->fragment_sleep, &Configs::SettingsRepo::fragment_sleep);
    ui->fragment_size->setValidator(new QRegularExpressionValidator(QRegularExpression("^[0-9]+(-[0-9]+)?$"), this));
    ui->fragment_sleep->setValidator(new QRegularExpressionValidator(QRegularExpression("^[0-9]+(-[0-9]+)?$"), this));
    // Remove hardcoded 80px maximum, derive from font metrics for widest expected input
    const int fragmentWidth = ui->fragment_size->fontMetrics().horizontalAdvance(QStringLiteral("000-000")) + 24;
    ui->fragment_size->setMaximumWidth(fragmentWidth);
    ui->fragment_sleep->setMaximumWidth(fragmentWidth);
    // size/sleep only affect the custom implementation, so enable them only for it
    auto syncFragParams = [this](const QString &impl) {
        bool custom = impl == "custom";
        ui->fragment_size->setEnabled(custom);
        ui->fragment_sleep->setEnabled(custom);
        ui->fragment_size_l->setEnabled(custom);
        ui->fragment_sleep_l->setEnabled(custom);
    };
    connect(ui->fragment_implementation, &QComboBox::currentTextChanged, this, syncFragParams);
    syncFragParams(ui->fragment_implementation->currentText());
    ui->dns_in_port->setValidator(new QIntValidator(1, 65535, ui->dns_in_port));
    ui->dns_in_port->setText(Int2String(Configs::dataManager->settingsRepo->core_dns_in_port));

    // Clash API (was behind a "Core Options" popup)
    ui->core_box_clash_listen_addr->setText(Configs::dataManager->settingsRepo->core_box_clash_listen_addr);
    ui->core_box_clash_api->setValidator(new QIntValidator(1, 65535, ui->core_box_clash_api));
    ui->core_box_clash_api->setText(Configs::dataManager->settingsRepo->core_box_clash_api > 0
                                        ? Int2String(Configs::dataManager->settingsRepo->core_box_clash_api)
                                        : "");
    ui->core_box_clash_api_secret->setText(Configs::dataManager->settingsRepo->core_box_clash_api_secret);

    // Xray
    ui->xray_mux_concurrency->setText(Int2String(Configs::dataManager->settingsRepo->xray_mux_concurrency));
    ui->xray_default_mux->setChecked(Configs::dataManager->settingsRepo->xray_mux_default_on);
    ui->vless_xray_pref->addItems(Configs::Xray::XrayVlessPreferenceString);
    ui->vless_xray_pref->setCurrentIndex(Configs::dataManager->settingsRepo->xray_vless_preference);
    dLoadText(ui->xray_geoip_url, &Configs::SettingsRepo::xray_geoip_url);
    dLoadText(ui->xray_geosite_url, &Configs::SettingsRepo::xray_geosite_url);
    ui->xray_geoip_url->setPlaceholderText("https://github.com/Loyalsoldier/v2ray-rules-dat/raw/release/geoip.dat");
    ui->xray_geosite_url->setPlaceholderText("https://github.com/Loyalsoldier/v2ray-rules-dat/raw/release/geosite.dat");

    // NTP
    ui->ntp_enable->setChecked(Configs::dataManager->settingsRepo->enable_ntp);
    ui->ntp_server->setEnabled(Configs::dataManager->settingsRepo->enable_ntp);
    ui->ntp_port->setEnabled(Configs::dataManager->settingsRepo->enable_ntp);
    ui->ntp_interval->setEnabled(Configs::dataManager->settingsRepo->enable_ntp);
    ui->ntp_outbound->setEnabled(Configs::dataManager->settingsRepo->enable_ntp);
    ui->ntp_server->setText(Configs::dataManager->settingsRepo->ntp_server_address);
    ui->ntp_port->setText(Int2String(Configs::dataManager->settingsRepo->ntp_server_port));
    ui->ntp_interval->setCurrentText(Configs::dataManager->settingsRepo->ntp_interval);
    ui->ntp_outbound->setCurrentText(Configs::dataManager->settingsRepo->ntp_outbound);
    connect(ui->ntp_enable, &QCheckBox::checkStateChanged, this, [this](Qt::CheckState state) {
        const bool on = state == Qt::Checked;
        ui->ntp_server->setEnabled(on);
        ui->ntp_port->setEnabled(on);
        ui->ntp_interval->setEnabled(on);
        ui->ntp_outbound->setEnabled(on);
    });

    // Security

    ui->utlsFingerprint->addItems(Configs::tlsFingerprints);
    ui->disable_priv_req->setChecked(Configs::dataManager->settingsRepo->disable_privilege_req);
    ui->windows_no_admin->setChecked(Configs::dataManager->settingsRepo->disable_run_admin);
    ui->mozilla_cert->setChecked(Configs::dataManager->settingsRepo->use_mozilla_certs);

    dLoadBool(ui->skip_cert, &Configs::SettingsRepo::skip_cert);
    ui->utlsFingerprint->setCurrentText(Configs::dataManager->settingsRepo->utlsFingerprint);

    // The .ui geometry is a design-time hint only: the size the content actually
    // needs depends on the platform's font metrics and on the active translation,
    // and both run larger than what the layout was drawn against. Held at the
    // designed size, the tab's rows get handed less height than their minimum and
    // Qt lays them out overlapping each other (#1671). Size to the content
    // instead, bounded by the screen so the button box stays reachable.
    QSize want = sizeHint();
    if (const QScreen *scr = parent ? parent->screen() : screen()) {
        const QRect avail = scr->availableGeometry();
        want = want.boundedTo(QSize(avail.width() - 24, avail.height() - 72));
    }
    resize(want);
}

DialogBasicSettings::~DialogBasicSettings() {
    delete ui;
}

void DialogBasicSettings::changeEvent(QEvent *event) {
    if (event->type() == QEvent::LanguageChange) {
        ui->retranslateUi(this);
        retranslateDynamicUi();
    }
    QDialog::changeEvent(event);
}

void DialogBasicSettings::retranslateDynamicUi() {
    const auto details = GetDeviceDetails();
    ui->sub_send_hwid->setToolTip(
        ui->sub_send_hwid->toolTip().arg(
            details.hwid.isEmpty() ? QStringLiteral("N/A") : details.hwid,
            details.os.isEmpty() ? QStringLiteral("N/A") : details.os,
            details.osVersion.isEmpty() ? QStringLiteral("N/A") : details.osVersion,
            details.model.isEmpty() ? QStringLiteral("N/A") : details.model));
}

static void highlightRegexLines(QTextEdit *edit) {
    if (!edit || !edit->document()) return;
    edit->blockSignals(true);
    QTextDocument *doc = edit->document();
    QRegularExpression validator;
    for (int i = 0; i < doc->blockCount(); ++i) {
        QTextBlock block = doc->findBlockByNumber(i);
        QString line = block.text();
        QTextBlockFormat fmt = block.blockFormat();
        if (line.trimmed().isEmpty()) {
            fmt.setBackground(Qt::NoBrush);
            QTextCursor cur(block);
            cur.setBlockFormat(fmt);
            continue;
        }
        validator.setPattern(line);
        fmt.setBackground(QBrush(validator.isValid() ? Qt::darkGreen : Qt::darkRed));
        QTextCursor cur(block);
        cur.setBlockFormat(fmt);
    }
    edit->blockSignals(false);
}

void DialogBasicSettings::applyRegexHighlighting() {
    highlightRegexLines(ui->log_include_regex);
    highlightRegexLines(ui->log_exclude_regex);
}

void DialogBasicSettings::accept() {
    // Common
    bool needChoosePort = false;
    const auto &settings = *Configs::dataManager->settingsRepo;
    const auto proxySettingsState = [&] {
        return std::make_tuple(
            settings.inbound_address,
            settings.custom_inbound,
            settings.inbound_socks_port,
            settings.disable_mixed_inbound,
            settings.inbound_auth,
            settings.inbound_user,
            settings.inbound_pass,
            settings.log_level,
            settings.xray_log_level,
            settings.disable_traffic_stats,
            settings.core_dns_in_port,
            settings.xray_mux_concurrency,
            settings.xray_mux_default_on,
            settings.xray_vless_preference,
            settings.mux_concurrency,
            settings.mux_protocol,
            settings.mux_padding,
            settings.mux_default_on,
            settings.fragment_implementation,
            settings.fragment_default_on,
            settings.fragment_size,
            settings.fragment_sleep,
            settings.tls_tricks_default_on,
            settings.enable_ntp,
            settings.ntp_server_address,
            settings.ntp_server_port,
            settings.ntp_interval,
            settings.ntp_outbound,
            settings.utlsFingerprint,
            settings.use_mozilla_certs);
    };
    const auto proxySettingsBefore = proxySettingsState();

    dSaveText(ui->inbound_address, &Configs::SettingsRepo::inbound_address);
    Configs::dataManager->settingsRepo->custom_inbound = CACHE.custom_inbound;
    dSaveInt(ui->inbound_socks_port, &Configs::SettingsRepo::inbound_socks_port);
    if (!Configs::dataManager->settingsRepo->random_inbound_port && ui->random_listen_port->isChecked())
    {
        needChoosePort = true;
    }
    Configs::dataManager->settingsRepo->random_inbound_port = ui->random_listen_port->isChecked();
    dSaveInt(ui->test_concurrent, &Configs::SettingsRepo::test_concurrent);
    dSaveText(ui->test_latency_url, &Configs::SettingsRepo::test_latency_url);
    dSaveBool(ui->disable_tray, &Configs::SettingsRepo::disable_tray);
    Configs::dataManager->settingsRepo->proxy_scheme = ui->proxy_scheme->currentText().toLower();
    Configs::dataManager->settingsRepo->speed_test_mode = ui->speedtest_mode->currentIndex();
    Configs::dataManager->settingsRepo->simple_dl_url = ui->simple_down_url->text();
    Configs::dataManager->settingsRepo->url_test_timeout_ms = ui->url_timeout->text().toInt();
    Configs::dataManager->settingsRepo->speed_test_timeout_ms = ui->test_timeout->text().toInt();
    Configs::dataManager->settingsRepo->allow_beta_update = ui->allow_beta->isChecked();
    Configs::dataManager->settingsRepo->disable_mixed_inbound = ui->disable_mixed_inbound->isChecked();
    Configs::dataManager->settingsRepo->reset_proxy_on_disable_sp = ui->reset_proxy_on_disable_sp->isChecked();
    dSaveBool(ui->inbound_auth, &Configs::SettingsRepo::inbound_auth);
    dSaveText(ui->inbound_user, &Configs::SettingsRepo::inbound_user);
    dSaveText(ui->inbound_pass, &Configs::SettingsRepo::inbound_pass);

    // Logging
    auto oldMaxLogLines = Configs::dataManager->settingsRepo->max_log_line;
    Configs::dataManager->settingsRepo->max_log_line = ui->max_log_line->text().toInt();
    if (oldMaxLogLines != Configs::dataManager->settingsRepo->max_log_line) CACHE.updateMaxLogLines = true;
    Configs::dataManager->settingsRepo->log_level = ui->log_level->currentText();
    Configs::dataManager->settingsRepo->xray_log_level = ui->xray_loglevel->currentText();
    Configs::dataManager->settingsRepo->log_enable_include = ui->enable_log_include->isChecked();
    Configs::dataManager->settingsRepo->log_enable_exclude = ui->enable_log_exclude->isChecked();
    dSaveBool(ui->log_auto_scroll, &Configs::SettingsRepo::log_auto_scroll);
    Configs::dataManager->settingsRepo->log_include_keyword = SplitAndTrim(ui->log_include_keyword->toPlainText(), "\n", false);
    Configs::dataManager->settingsRepo->log_exclude_keyword = SplitAndTrim(ui->log_exclude_keyword->toPlainText(), "\n", false);

    Configs::dataManager->settingsRepo->log_include_regex.clear();
    Configs::dataManager->settingsRepo->log_exclude_regex.clear();
    QRegularExpression regexValidator;
    for (QStringList log_include_lines = SplitAndTrim(ui->log_include_regex->toPlainText(), "\n", false); const QString &line : log_include_lines) {
        if (regexValidator.setPattern(line); regexValidator.isValid()) Configs::dataManager->settingsRepo->log_include_regex << line;
    }
    for (QStringList log_exclude_lines = SplitAndTrim(ui->log_exclude_regex->toPlainText(), "\n", false); const QString &line : log_exclude_lines) {
        if (regexValidator.setPattern(line); regexValidator.isValid()) Configs::dataManager->settingsRepo->log_exclude_regex << line;
    }

    // Style

    Configs::dataManager->settingsRepo->enable_stats = ui->connection_statistics->isChecked();
    Configs::dataManager->settingsRepo->disable_traffic_aggregation = ui->disable_traffic_aggregation->isChecked();
    const bool languageChanged =
        Configs::dataManager->settingsRepo->language != ui->language->currentIndex();
    Configs::dataManager->settingsRepo->language = ui->language->currentIndex();
    auto oldUseCustomIcon = Configs::dataManager->settingsRepo->use_custom_icons;
    Configs::dataManager->settingsRepo->use_custom_icons = ui->enable_custom_icon->isChecked();
    if (oldUseCustomIcon != Configs::dataManager->settingsRepo->use_custom_icons) CACHE.updateTrayIcon = true;
    dSaveBool(ui->start_minimal, &Configs::SettingsRepo::start_minimal);
    Configs::dataManager->settingsRepo->skip_delete_confirmation = ui->skip_delete_confirm->isChecked();
    bool profileListDisplayChanged =
        Configs::dataManager->settingsRepo->show_config_security != ui->show_config_security->isChecked();
    dSaveBool(ui->show_config_security, &Configs::SettingsRepo::show_config_security);
    Configs::dataManager->settingsRepo->show_system_dns = ui->show_sys_dns->isChecked();

    if (Configs::dataManager->settingsRepo->max_log_line <= 0) {
        Configs::dataManager->settingsRepo->max_log_line = 200;
    }

    // Subscription
    // Intervals are just persisted here; the PeriodicRunner reads them live and is
    // re-checked from the UpdateSettings handler, so no timer needs restarting.

    Configs::dataManager->settingsRepo->user_agent = ui->user_agent->text();
    dSaveBool(ui->net_use_proxy, &Configs::SettingsRepo::net_use_proxy);
    dSaveBool(ui->sub_clear, &Configs::SettingsRepo::sub_clear);
    dSaveBool(ui->sub_show_change_popup, &Configs::SettingsRepo::sub_show_change_popup);
    dSaveBool(ui->net_insecure, &Configs::SettingsRepo::net_insecure);
    dSaveBool(ui->sub_send_hwid, &Configs::SettingsRepo::sub_send_hwid);
    dSaveBool(ui->allow_stopping_active_profile, &Configs::SettingsRepo::allow_stopping_active_profile);
    dSaveText(ui->sub_custom_hwid_params, &Configs::SettingsRepo::sub_custom_hwid_params);
    dSaveIntEnable(ui->sub_auto_update, ui->sub_auto_update_enable, &Configs::SettingsRepo::sub_auto_update);
    dSaveIntEnable(ui->route_auto_update, ui->route_auto_update_enable, &Configs::SettingsRepo::route_auto_update);

    // Core
    Configs::dataManager->settingsRepo->disable_traffic_stats = ui->disable_stats->isChecked();
    Configs::dataManager->settingsRepo->core_dns_in_port = ui->dns_in_port->text().toInt();
    Configs::dataManager->settingsRepo->core_box_clash_listen_addr = ui->core_box_clash_listen_addr->text();
    Configs::dataManager->settingsRepo->core_box_clash_api = ui->core_box_clash_api->text().toInt();
    Configs::dataManager->settingsRepo->core_box_clash_api_secret = ui->core_box_clash_api_secret->text();

    // Xray
    Configs::dataManager->settingsRepo->xray_mux_concurrency = ui->xray_mux_concurrency->text().toInt();
    Configs::dataManager->settingsRepo->xray_mux_default_on = ui->xray_default_mux->isChecked();
    Configs::dataManager->settingsRepo->xray_vless_preference = static_cast<Configs::Xray::XrayVlessPreference>(ui->vless_xray_pref->currentIndex());
    dSaveText(ui->xray_geoip_url, &Configs::SettingsRepo::xray_geoip_url);
    dSaveText(ui->xray_geosite_url, &Configs::SettingsRepo::xray_geosite_url);

    // Mux
    dSaveInt(ui->mux_concurrency, &Configs::SettingsRepo::mux_concurrency);
    dSaveComboText(ui->mux_protocol, &Configs::SettingsRepo::mux_protocol);
    dSaveBool(ui->mux_padding, &Configs::SettingsRepo::mux_padding);
    dSaveBool(ui->mux_default_on, &Configs::SettingsRepo::mux_default_on);
    dSaveComboText(ui->fragment_implementation, &Configs::SettingsRepo::fragment_implementation);
    dSaveBool(ui->fragment_default_on, &Configs::SettingsRepo::fragment_default_on);
    dSaveBool(ui->tls_tricks_default_on, &Configs::SettingsRepo::tls_tricks_default_on);
    dSaveText(ui->fragment_size, &Configs::SettingsRepo::fragment_size);
    dSaveText(ui->fragment_sleep, &Configs::SettingsRepo::fragment_sleep);

    // NTP
    Configs::dataManager->settingsRepo->enable_ntp = ui->ntp_enable->isChecked();
    Configs::dataManager->settingsRepo->ntp_server_address = ui->ntp_server->text();
    Configs::dataManager->settingsRepo->ntp_server_port = ui->ntp_port->text().toInt();
    Configs::dataManager->settingsRepo->ntp_interval = ui->ntp_interval->currentText();
    Configs::dataManager->settingsRepo->ntp_outbound = ui->ntp_outbound->currentText();

    // Security

    dSaveBool(ui->skip_cert, &Configs::SettingsRepo::skip_cert);
    Configs::dataManager->settingsRepo->utlsFingerprint = ui->utlsFingerprint->currentText();
    Configs::dataManager->settingsRepo->disable_privilege_req = ui->disable_priv_req->isChecked();
    if (Configs::dataManager->settingsRepo->disable_run_admin != ui->windows_no_admin->isChecked()) CACHE.updateDisableAdmin = true;
    Configs::dataManager->settingsRepo->disable_run_admin = ui->windows_no_admin->isChecked();
    Configs::dataManager->settingsRepo->use_mozilla_certs = ui->mozilla_cert->isChecked();

    const bool proxySettingsChanged =
        proxySettingsBefore != proxySettingsState();

    QStringList changes;
    if (proxySettingsChanged) changes << MwArg::RestartProxy;
    if (languageChanged) changes << MwArg::Language;
    if (CACHE.updateDisableTray) changes << MwArg::DisableTray;
    if (CACHE.updateSystemDns) changes << MwArg::SystemDns;
    if (CACHE.updateTrayIcon) changes << MwArg::TrayIcon;
    if (CACHE.updateMaxLogLines) changes << MwArg::MaxLogLines;
    if (CACHE.updateDisableAdmin) changes << MwArg::DisableAdmin;
    if (needChoosePort) changes << MwArg::ChoosePort;
    if (profileListDisplayChanged) changes << MwArg::ProfileListDisplay;
    MW_dialog_message(MwMessage::UpdateSettings, changes);
    QDialog::accept();
}

// Backup archive format:
//   [magic: "THRN" 4 bytes]
//   [format_version: quint32]  -- identifies archive structure, increment on breaking layout changes
//   [metadata: QString]        -- compact JSON: backup_version, created_at, platform, parts
//   [files: QMap<QString,QByteArray>]  -- optional "database" + optional "icons/<name>" entries
// v1: full database snapshot + icons (no "parts" metadata; treated as all parts present).
// v2: selective database snapshot (only chosen categories retained) + "parts" metadata
//     describing which of profiles/routes/settings/icons the file contains.
static constexpr int BACKUP_CONTENT_VERSION = 2;

void DialogBasicSettings::downloadXrayGeoAsset(const QString &url, const QString &fileName) {
    const QString effectiveUrl = url.trimmed();
    if (effectiveUrl.isEmpty()) {
        QMessageBox::warning(this, tr("Download geo asset"),
            tr("Please enter a URL for %1 first.").arg(fileName));
        return;
    }
    MW_show_log(tr("Downloading Xray geo asset: %1").arg(fileName));
    // DownloadAsset drives a blocking event loop and reports progress through the
    // main window's data view, so it must run off the UI thread. Don't capture the
    // dialog — it may be closed before the download finishes; report through the
    // (long-lived) main window instead.
    runOnNewThread([effectiveUrl, fileName] {
        const auto err = NetworkRequestHelper::DownloadAsset(effectiveUrl, fileName);
        runOnUiThread([err, fileName] {
            if (err.isEmpty()) {
                MW_show_log(QObject::tr("Downloaded Xray geo asset: %1").arg(fileName));
                QMessageBox::information(GetMainWindow(), QObject::tr("Download geo asset"),
                    QObject::tr("%1 was downloaded successfully.").arg(fileName));
            } else {
                MessageBoxWarning(QObject::tr("Download geo asset"),
                    QObject::tr("Failed to download %1:\n%2").arg(fileName, err));
            }
        });
    });
}

void DialogBasicSettings::on_xray_geoip_download_clicked() {
    QString url = ui->xray_geoip_url->text().trimmed();
    if (url.isEmpty()) url = ui->xray_geoip_url->placeholderText();
    downloadXrayGeoAsset(url, "geoip.dat");
}

void DialogBasicSettings::on_xray_geosite_download_clicked() {
    QString url = ui->xray_geosite_url->text().trimmed();
    if (url.isEmpty()) url = ui->xray_geosite_url->placeholderText();
    downloadXrayGeoAsset(url, "geosite.dat");
}

void DialogBasicSettings::on_backup_create_clicked() {
    Configs::BackupParts parts;
    parts.profiles = ui->backup_inc_profiles->isChecked();
    parts.routes = ui->backup_inc_routes->isChecked();
    parts.settings = ui->backup_inc_settings->isChecked();
    parts.icons = ui->backup_inc_icons->isChecked();

    if (!parts.any()) {
        QMessageBox::warning(this, tr("Create Backup"),
            tr("Select at least one part to include in the backup."));
        return;
    }

    // Persist current in-memory settings so the snapshot reflects them.
    if (parts.settings && !Configs::dataManager->settingsRepo->Save()) {
        QMessageBox::critical(this, tr("Backup Failed"),
            tr("Failed to persist current settings before creating the backup."));
        return;
    }

    QString filePath = QFileDialog::getSaveFileName(
        this,
        tr("Create Backup"),
        QDir::homePath() + "/Throne-backup.thrbackup",
        tr("Throne Backup (*.thrbackup)")
    );
    if (filePath.isEmpty()) return;

    QJsonObject partsObj;
    partsObj["profiles"] = parts.profiles;
    partsObj["routes"] = parts.routes;
    partsObj["settings"] = parts.settings;
    partsObj["icons"] = parts.icons;

    QJsonObject meta;
    meta["backup_version"] = BACKUP_CONTENT_VERSION;
    meta["created_at"] = QDateTime::currentDateTime().toString(Qt::TextDate);
    meta["platform"] = QSysInfo::kernelType();
    meta["parts"] = partsObj;

    QMap<QString, QByteArray> files;
    QMap<QString, qint64> entrySizes;
    const qint64 metadataBytes = BackupArchive::metadataSize(meta);
    QString archiveError;

    // Size/name preflight before any payload is loaded into memory.
    if (parts.icons) {
        QDir iconsDir("icons");
        if (iconsDir.exists()) {
            for (const QFileInfo& entry : iconsDir.entryInfoList(QDir::Files)) {
                const QString key = QStringLiteral("icons/") + entry.fileName();
                if (BackupArchive::portableIconKey(key).isEmpty()) {
                    QMessageBox::critical(this, tr("Backup Failed"),
                        tr("Custom icon name is not portable: %1").arg(entry.fileName()));
                    return;
                }
                entrySizes.insert(key, entry.size());
            }
        }
    }

    QTemporaryDir tempDir;
    QString tempDbPath;
    if (parts.anyDb()) {
        if (!tempDir.isValid()) {
            QMessageBox::critical(this, tr("Backup Failed"), tr("Failed to create temporary directory."));
            return;
        }
        tempDbPath = tempDir.filePath("backup.db");
        try {
            Configs::dataManager->getDatabase().backupSelective(tempDbPath.toStdString(), parts);
        } catch (std::exception& e) {
            QMessageBox::critical(this, tr("Backup Failed"),
                tr("Failed to create database snapshot: %1").arg(e.what()));
            return;
        }
        entrySizes.insert(QStringLiteral("database"), QFileInfo(tempDbPath).size());
    }

    if (!BackupArchive::validateEntrySizes(entrySizes, metadataBytes, &archiveError)) {
        QMessageBox::critical(this, tr("Backup Failed"), archiveError);
        return;
    }

    if (parts.anyDb()) {
        QFile tempDbFile(tempDbPath);
        if (!tempDbFile.open(QIODevice::ReadOnly)) {
            QMessageBox::critical(this, tr("Backup Failed"), tr("Failed to read database snapshot."));
            return;
        }
        const QByteArray data = tempDbFile.readAll();
        if (tempDbFile.error() != QFileDevice::NoError ||
            data.size() != entrySizes.value(QStringLiteral("database"))) {
            QMessageBox::critical(this, tr("Backup Failed"), tr("Failed to read database snapshot."));
            return;
        }
        files.insert(QStringLiteral("database"), data);
    }

    for (auto it = entrySizes.constBegin(); it != entrySizes.constEnd(); ++it) {
        if (!it.key().startsWith(QStringLiteral("icons/"))) continue;
        QFile iconFile(QStringLiteral("icons/") + it.key().mid(6));
        if (!iconFile.open(QIODevice::ReadOnly)) {
            QMessageBox::critical(this, tr("Backup Failed"),
                tr("Failed to read custom icon: %1").arg(it.key().mid(6)));
            return;
        }
        const QByteArray data = iconFile.readAll();
        if (iconFile.error() != QFileDevice::NoError || data.size() != it.value()) {
            QMessageBox::critical(this, tr("Backup Failed"),
                tr("Failed to read custom icon: %1").arg(it.key().mid(6)));
            return;
        }
        files.insert(it.key(), data);
    }

    QSaveFile outFile(filePath);
    if (!outFile.open(QIODevice::WriteOnly)) {
        QMessageBox::critical(this, tr("Backup Failed"),
            tr("Cannot write to: %1").arg(filePath));
        return;
    }

    if (!BackupArchive::write(outFile, meta, files, &archiveError) || !outFile.commit()) {
        if (archiveError.isEmpty()) archiveError = outFile.errorString();
        QMessageBox::critical(this, tr("Backup Failed"), archiveError);
        return;
    }

    QStringList included;
    if (parts.profiles) included << tr("Profiles");
    if (parts.routes) included << tr("Routing profiles");
    if (parts.settings) included << tr("Settings");
    if (parts.icons) included << tr("Custom icons");

    QMessageBox::information(this, tr("Backup Created"),
        tr("Backup created successfully:\n%1\n\nIncluded: %2")
            .arg(filePath, included.join(", ")));
}

void DialogBasicSettings::on_backup_restore_clicked() {
    QString filePath = QFileDialog::getOpenFileName(
        this,
        tr("Restore Backup"),
        QDir::homePath(),
        tr("Throne Backup (*.thrbackup)")
    );
    if (filePath.isEmpty()) return;

    QFile inFile(filePath);
    if (!inFile.open(QIODevice::ReadOnly)) {
        QMessageBox::critical(this, tr("Restore Failed"),
            tr("Cannot open backup file: %1").arg(filePath));
        return;
    }

    BackupArchive::Archive archive;
    QString archiveError;
    if (!BackupArchive::read(inFile, &archive, &archiveError)) {
        QMessageBox::critical(this, tr("Restore Failed"), archiveError);
        return;
    }
    inFile.close();

    BackupArchive::Parts availableParts;
    if (!BackupArchive::partsForRestore(archive, &availableParts, &archiveError)) {
        QMessageBox::critical(this, tr("Restore Failed"), archiveError);
        return;
    }

    const QJsonObject& meta = archive.metadata;
    const QMap<QString, QByteArray>& files = archive.files;
    const QString createdAt = meta["created_at"].toString();

    Configs::BackupParts avail;
    avail.profiles = availableParts.profiles;
    avail.routes = availableParts.routes;
    avail.settings = availableParts.settings;
    avail.icons = availableParts.icons;
    if (!avail.any()) {
        QMessageBox::critical(this, tr("Restore Failed"),
            tr("This backup file does not contain any restorable data."));
        return;
    }

    // Let the user pick which of the available parts to restore.
    QDialog dlg(this);
    dlg.setWindowTitle(tr("Restore Backup"));
    auto* layout = new QVBoxLayout(&dlg);
    auto* header = new QLabel(
        tr("Backup created on %1.\nSelect which parts to restore:")
            .arg(createdAt.isEmpty() ? tr("unknown date") : createdAt), &dlg);
    header->setWordWrap(true);
    layout->addWidget(header);

    auto* cbProfiles = new QCheckBox(tr("Profiles (groups and proxies)"), &dlg);
    auto* cbRoutes = new QCheckBox(tr("Routing profiles"), &dlg);
    auto* cbSettings = new QCheckBox(tr("Settings"), &dlg);
    auto* cbIcons = new QCheckBox(tr("Custom icons"), &dlg);
    for (auto* cb : {cbProfiles, cbRoutes, cbSettings, cbIcons}) cb->setChecked(true);
    cbProfiles->setEnabled(avail.profiles);
    cbProfiles->setChecked(avail.profiles);
    cbRoutes->setEnabled(avail.routes);
    cbRoutes->setChecked(avail.routes);
    cbSettings->setEnabled(avail.settings);
    cbSettings->setChecked(avail.settings);
    cbIcons->setEnabled(avail.icons);
    cbIcons->setChecked(avail.icons);
    layout->addWidget(cbProfiles);
    layout->addWidget(cbRoutes);
    layout->addWidget(cbSettings);
    layout->addWidget(cbIcons);

    auto* warn = new QLabel(
        tr("Each selected part replaces the current data. This cannot be undone.\n"
           "Throne will restart to complete the restore."), &dlg);
    warn->setWordWrap(true);
    layout->addWidget(warn);

    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dlg);
    buttons->button(QDialogButtonBox::Ok)->setText(tr("Restore"));
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, &dlg, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dlg, &QDialog::reject);

    if (dlg.exec() != QDialog::Accepted) return;

    Configs::BackupParts chosen;
    chosen.profiles = avail.profiles && cbProfiles->isChecked();
    chosen.routes = avail.routes && cbRoutes->isChecked();
    chosen.settings = avail.settings && cbSettings->isChecked();
    chosen.icons = avail.icons && cbIcons->isChecked();

    if (!chosen.any()) {
        QMessageBox::warning(this, tr("Restore Backup"),
            tr("Select at least one part to restore."));
        return;
    }

    BackupRestore::Request request;
    request.rootPath = QDir::currentPath();
    request.database = chosen.anyDb();
    request.icons = chosen.icons;
    request.files = files;

    // Profile restore replaces the rows that the traffic loop periodically
    // updates. Join that writer before applying the snapshot so stale in-memory
    // totals cannot be written back after the restore commits.
    const bool resumeTrafficOnFailure = chosen.profiles && Stats::trafficLooper->IsRunning();
    const bool trafficWasEnabled = Stats::trafficLooper->loop_enabled.load(std::memory_order_acquire);
    if (chosen.profiles) Stats::trafficLooper->Stop();

    BackupRestore::DatabaseActions databaseActions;
    if (chosen.anyDb()) {
        auto& database = Configs::dataManager->getDatabase();
        databaseActions.backup = [&database](const QString& path) {
            database.backupTo(path.toStdString());
        };
        databaseActions.apply = [&database, chosen](const QString& path) {
            database.restoreSelective(path.toStdString(), chosen);
        };
        databaseActions.rollback = [&database](const QString& path) {
            database.restoreFrom(path.toStdString());
        };
    }

    const BackupRestore::Result restoreResult =
        BackupRestore::execute(request, databaseActions);
    if (!restoreResult.success) {
        if (restoreResult.recoveryPending) {
            Configs::dataManager->settingsRepo->noSave = true;
            QMessageBox::critical(this, tr("Restore Recovery Required"),
                tr("The backup restore could not be rolled back safely. Throne will restart "
                   "and recover the previous data before opening the database.\n\n%1")
                    .arg(restoreResult.error));
            MW_dialog_message(MwMessage::RestartProgram, {});
            QDialog::reject();
        } else {
            if (resumeTrafficOnFailure) {
                Stats::trafficLooper->loop_enabled.store(trafficWasEnabled, std::memory_order_release);
                Stats::trafficLooper->Start();
            }
            QMessageBox::critical(this, tr("Restore Failed"), restoreResult.error);
        }
        return;
    }

    // The in-memory SettingsRepo still holds the pre-restore values. The restart
    // path runs prepare_exit() -> on_commitDataRequest() -> settingsRepo->Save(),
    // which would write those stale values straight back over the freshly
    // restored settings table. Suppress that save so the restore survives.
    if (chosen.settings) Configs::dataManager->settingsRepo->noSave = true;

    QMessageBox::information(this, tr("Restore Complete"),
        tr("Backup restored successfully. Throne will now restart for the changes to take effect."));
    MW_dialog_message(MwMessage::RestartProgram, {});
    QDialog::reject();
}

