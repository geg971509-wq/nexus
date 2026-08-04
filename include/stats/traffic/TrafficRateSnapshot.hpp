#pragma once

namespace Stats {
    // Immutable four-rate sample published for cross-thread UI reads.
    struct TrafficRateSnapshot {
        double proxy_downlink = 0;
        double proxy_uplink = 0;
        double direct_downlink = 0;
        double direct_uplink = 0;
    };
} // namespace Stats
