#pragma once
#include <QString>

#include <memory>

namespace Configs
{
    class outbound;

    // Concrete outbound for a Throne type string; unknown types yield a base
    // outbound flagged invalid. Never null.
    [[nodiscard]] std::shared_ptr<outbound> NewOutboundByType(const QString& type);
}
