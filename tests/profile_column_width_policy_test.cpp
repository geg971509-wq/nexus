#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

#include <cstdlib>
#include <iostream>

#ifndef THRONE_SOURCE_DIR
#error THRONE_SOURCE_DIR must be defined
#endif

namespace {
QString readSource() {
  QFile file(QStringLiteral(THRONE_SOURCE_DIR "/src/ui/mainwindow.cpp"));
  if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
    return {};
  return QString::fromUtf8(file.readAll());
}

QString sizingFunction(const QString &source) {
  const qsizetype begin = source.indexOf(
      QStringLiteral("void MainWindow::refresh_proxy_list_column_size() {"));
  const qsizetype end = source.indexOf(
      QStringLiteral("\nvoid MainWindow::refresh_proxy_list("), begin);
  if (begin < 0 || end < 0)
    return {};
  return source.mid(begin, end - begin);
}

QString refreshAction(const QString &source) {
  const qsizetype begin =
      source.indexOf(QStringLiteral("connect(ui->actionRefresh_Column_Widths"));
  const qsizetype end = source.indexOf(QStringLiteral("\n    });"), begin);
  if (begin < 0 || end < 0)
    return {};
  return source.mid(begin, end - begin);
}

QString languageChangeAction(const QString &source) {
  const qsizetype begin = source.indexOf(
      QStringLiteral("if (event->type() == QEvent::LanguageChange) {"));
  const qsizetype end = source.indexOf(
      QStringLiteral("\n    if (event->type() == QEvent::FontChange)"), begin);
  if (begin < 0 || end < 0)
    return {};
  return source.mid(begin, end - begin);
}
} // namespace

int main() {
  const QString source = readSource();
  const QString sizing = sizingFunction(source);
  const QString refresh = refreshAction(source);
  const QString languageChange = languageChangeAction(source);

  QJsonObject checks;
  checks.insert(
      QStringLiteral("all_five_columns"),
      sizing.contains(QStringLiteral("ProfilesTableModel::ColumnCount")) &&
          sizing.contains(
              QStringLiteral("for (int col = 0; col < columnCount; ++col)")));
  checks.insert(
      QStringLiteral("full_source_model_rows"),
      sizing.contains(QStringLiteral("profilesTableModel->rowCount()")) &&
          sizing.contains(
              QStringLiteral("for (int row = 0; row < rows; ++row)")) &&
          sizing.contains(QStringLiteral("profilesTableModel->data(")));
  checks.insert(
      QStringLiteral("translated_headers"),
      sizing.contains(QStringLiteral("profilesTableModel->headerData(")));
  checks.insert(
      QStringLiteral("max_header_and_cells"),
      sizing.contains(QStringLiteral(
          "headerMetrics.horizontalAdvance(header) + widthAllowance")) &&
          sizing.contains(QStringLiteral(
              "cellMetrics.horizontalAdvance(text) + widthAllowance")) &&
          sizing.count(QStringLiteral("qMax(")) >= 2);
  checks.insert(QStringLiteral("no_filter_proxy"),
                !sizing.contains(QStringLiteral("profilesFilterModel")));
  checks.insert(QStringLiteral("no_stretch"),
                !sizing.contains(QStringLiteral("QHeaderView::Stretch")));
  checks.insert(QStringLiteral("horizontal_overflow_as_needed"),
                !sizing.contains(QStringLiteral("Qt::ScrollBarAlwaysOff")) &&
                    sizing.count(QStringLiteral("Qt::ScrollBarAsNeeded")) >= 2);
  checks.insert(QStringLiteral("manual_width_restore"),
                sizing.contains(QStringLiteral("group->column_width.at(i)")));
  checks.insert(
      QStringLiteral("calculated_width_cache"),
      sizing.contains(QStringLiteral(
          "group->calculated_column_width.resize(columnCount)")) &&
          sizing.contains(
              QStringLiteral("group->calculated_column_width[col]")) &&
          sizing.contains(QStringLiteral(
              "if (group->calculated_column_width[col] > 0) continue;")) &&
          !sizing.contains(
              QStringLiteral("group->calculated_column_width.clear()")));
  checks.insert(QStringLiteral("refresh_clears_both_caches"),
                refresh.contains(QStringLiteral("ent->column_width.clear()")) &&
                    refresh.contains(
                        QStringLiteral("ent->clearCalculatedColumnWidth()")));
  checks.insert(
      QStringLiteral("language_change_remeasures_auto_widths"),
      languageChange.indexOf(
          QStringLiteral("profilesTableModel->retranslateHeaders()")) <
              languageChange.indexOf(
                  QStringLiteral("group->clearCalculatedColumnWidth()")) &&
          languageChange.indexOf(
              QStringLiteral("group->clearCalculatedColumnWidth()")) <
              languageChange.indexOf(
                  QStringLiteral("refresh_proxy_list_column_size()")) &&
          !languageChange.contains(QStringLiteral("column_width.clear()")));

  int passed = 0;
  for (auto it = checks.cbegin(); it != checks.cend(); ++it) {
    if (it.value().toBool())
      ++passed;
  }
  const bool pass = !checks.isEmpty() && passed == checks.size();
  const int score = checks.isEmpty() ? 0 : (passed * 100) / checks.size();
  const QJsonObject result{
      {QStringLiteral("pass"), pass},
      {QStringLiteral("score"), score},
      {QStringLiteral("checks"), checks},
  };
  std::cout << QJsonDocument(result).toJson(QJsonDocument::Compact).constData();
  return pass ? EXIT_SUCCESS : EXIT_FAILURE;
}
