#pragma once

#include <QWidget>
#include "profile_editor.h"
#include "ui_edit_autoselector.h"

QT_BEGIN_NAMESPACE
namespace Ui {
    class EditAutoSelector;
}
QT_END_NAMESPACE

class EditAutoSelector : public QWidget, public ProfileEditor {
    Q_OBJECT

public:
    explicit EditAutoSelector(QWidget *parent = nullptr);

    ~EditAutoSelector() override;

    void onStart(std::shared_ptr<Configs::Profile> _ent) override;

    bool onEnd() override;

private:
    Ui::EditAutoSelector *ui;
    std::shared_ptr<Configs::Profile> ent;
    // Suppresses the plan preview while onStart populates the form.
    bool m_loading = false;

    // Re-resolves membership and writes the human-readable summary, so the
    // effect of a filter or cap is visible before saving.
    void refreshPlanSummary();

    void updateBalanceEnabled() const;

    void mirrorTooltipsToLabels() const;

    // Re-fits the parent dialog after the advanced area is shown or hidden.
    void resizeDialogToContent();
};
