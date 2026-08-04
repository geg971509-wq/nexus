#pragma once

#include <QString>
#include "include/global/HTTPRequestHelper.hpp"
#ifndef Q_MOC_RUN
#include <core/server/gen/libcore.pb.h>
#endif

class DataViewHtmlGenerator {
public:
    struct DownloadPanelState {
        bool visible = false;
        DownloadProgressReport report;
    };

    struct SpeedtestPanelState {
        enum class Kind { Speed, Country };
        bool visible = false;
        Kind kind = Kind::Speed;
        QString profileName;
        QString dlSpeed;
        QString ulSpeed;
        QString serverCountryFlag;
        QString serverCountry;
        QString serverName;
        int totalProfiles = 0;
    };

    struct LatencyTestPanelState {
        enum class Kind { Url, Ip };
        bool visible = false;
        Kind kind = Kind::Url;
        int totalProfiles = 0;
    };

    // Lowest priority of all panels: it reports steady state rather than a
    // running job, so anything transient takes the space instead.
    struct AutoSelectorPanelState {
        bool visible = false;
        QString summary;
        QString detail;
    };

    void setDownloadReport(const DownloadProgressReport &report, bool show);

    void seedSpeedTest(int totalProfiles);

    void setSpeedtestProgress(const QString &profileName, const libcore::SpeedTestResult &result);

    void seedLatencyTest(LatencyTestPanelState::Kind kind, int totalProfiles);

    // Empty summary hides the panel.
    void setAutoSelectorStatus(const QString &summary, const QString &detail);

    void clearTestSections();

    void addTestProgress(int count = 1);

    QString buildHtml();

private:
    static QString getProgressBar(long long current, long long total);

    QString downloadSectionHtml();

    QString speedtestSectionHtml();

    QString latencyTestSectionHtml();

    QString autoSelectorSectionHtml();

    DownloadPanelState download_ = {};
    SpeedtestPanelState speedtest_ = {};
    LatencyTestPanelState latencyTest_ = {};
    AutoSelectorPanelState autoSelector_ = {};

    std::atomic<int> testProgress;
};
