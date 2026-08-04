#pragma once

#include <QDialog>

#include "include/stats/autoselector/AutoSelectorMonitor.hpp"

class QLabel;
class QTableWidget;
class QPushButton;
class QCheckBox;

// Live view of the running auto selector: what it picked, how every member is
// doing, and why. Opened from Tools; the action only exists while a selector
// profile is running.
class DialogAutoSelector : public QDialog {
    Q_OBJECT

public:
    explicit DialogAutoSelector(QWidget *parent = nullptr);

    // Pulls a fresh snapshot from the monitor. Safe to call on every poll.
    void refresh();

private:
    void buildRows(const Stats::AutoSelectorView &view);

    // Sorting is done here rather than by QTableWidget: its built-in sort
    // compares the displayed strings, which would order "100 ms" before "20 ms".
    void sortRows(QList<Stats::AutoSelectorMemberView> &rows) const;

    void onHeaderClicked(int column);

    // Widens the dialog to whatever the nine columns actually need.
    void fitToColumns();

    QLabel *m_headline = nullptr;
    QLabel *m_detail = nullptr;
    QLabel *m_footer = nullptr;
    QTableWidget *m_table = nullptr;
    QPushButton *m_recheck = nullptr;
    QCheckBox *m_onlyProblems = nullptr;

    int m_sortColumn = 0;
    Qt::SortOrder m_sortOrder = Qt::AscendingOrder;
    // Columns are auto-sized once; after that the user's own widths stick.
    bool m_columnsSized = false;
};
