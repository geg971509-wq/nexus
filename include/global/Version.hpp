#pragma once

#include <QString>

namespace Throne {

[[nodiscard]] bool IsVersionNewer(const QString &candidate, const QString &current);

}
