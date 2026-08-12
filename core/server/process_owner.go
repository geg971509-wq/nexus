package main

import (
	"context"
	"net"
	"net/netip"
	"sync"
	"time"

	"NexusCore/internal/boxbox"

	"github.com/sagernet/sing-box/adapter"
	sboxprocess "github.com/sagernet/sing-box/common/process"
	"github.com/sagernet/sing-box/experimental/clashapi"
	"github.com/sagernet/sing-box/experimental/clashapi/trafficontrol"
	M "github.com/sagernet/sing/common/metadata"
	N "github.com/sagernet/sing/common/network"
	"github.com/sagernet/sing/service"
)

// processOwnerEnricher runs after clash traffic tracking joins the conn.
// Strategy (maximize path + pid coverage):
//  1. Prefer ProcessInfo already filled by route find_process.
//  2. Else look up via throng darwin/linux/windows searcher (multi dest candidates).
//  3. If only pid is known, fill ProcessPath via processPathFromPID (root under Tun).
//  4. Short deferred retries when PCB entry is not ready at route time.
//
// Never a continuous poll — bounded attempts only.
type processOwnerEnricher struct{}

var (
	processSearcherOnce sync.Once
	processSearcher     sboxprocess.Searcher
)

func getProcessSearcher() sboxprocess.Searcher {
	processSearcherOnce.Do(func() {
		searcher, err := sboxprocess.NewSearcher(sboxprocess.Config{})
		if err == nil {
			processSearcher = searcher
		}
	})
	return processSearcher
}

func processPathOf(c *trafficontrol.TrackerMetadata) string {
	if c == nil || c.Metadata.ProcessInfo == nil {
		return ""
	}
	return c.Metadata.ProcessInfo.ProcessPath
}

// processOwnerKnown: path and/or OS pid — path may fail (SIP) while pid is still valid.
func processOwnerKnown(info *adapter.ConnectionOwner) bool {
	return info != nil && (info.ProcessPath != "" || info.ProcessID > 0)
}

func trackerHasOwner(c *trafficontrol.TrackerMetadata) bool {
	if c == nil || c.Metadata.ProcessInfo == nil {
		return false
	}
	return processOwnerKnown(c.Metadata.ProcessInfo)
}

// enrichOwnerPath: when we have pid but empty path, resolve via OS (root helps under Tun).
func enrichOwnerPath(info *adapter.ConnectionOwner) *adapter.ConnectionOwner {
	if info == nil {
		return nil
	}
	if info.ProcessPath != "" || info.ProcessID == 0 {
		return info
	}
	if path := processPathFromPID(info.ProcessID); path != "" {
		// Copy so we never mutate shared cache entries from route/searcher.
		out := *info
		out.ProcessPath = path
		return &out
	}
	return info
}

func ownerRicher(a, b *adapter.ConnectionOwner) *adapter.ConnectionOwner {
	// Prefer the one with path; else higher pid presence.
	if a == nil {
		return b
	}
	if b == nil {
		return a
	}
	aPath, bPath := a.ProcessPath != "", b.ProcessPath != ""
	if aPath && !bPath {
		return a
	}
	if bPath && !aPath {
		return b
	}
	if a.ProcessID > 0 && b.ProcessID == 0 {
		return a
	}
	if b.ProcessID > 0 && a.ProcessID == 0 {
		return b
	}
	// both same richness — prefer longer path (more complete)
	if len(b.ProcessPath) > len(a.ProcessPath) {
		return b
	}
	return a
}

func socksaddrFromNet(addr net.Addr) M.Socksaddr {
	if addr == nil {
		return M.Socksaddr{}
	}
	return M.SocksaddrFromNet(addr).Unwrap()
}

func addrPortOf(sa M.Socksaddr) netip.AddrPort {
	if !sa.IsValid() || !sa.IsIP() {
		return netip.AddrPort{}
	}
	return sa.AddrPort()
}

// exactSocketTuple prefers the real accepted socket endpoints. For mixed /
// system-proxy, RemoteAddr is the client and LocalAddr is our listen port —
// that is the PCB entry we must match. Metadata destinations are the logical
// remote and are still tried for TUN.
func exactSocketTuple(conn net.Conn, metadata adapter.InboundContext) (source netip.AddrPort, dests []netip.AddrPort) {
	seen := map[netip.AddrPort]struct{}{}
	add := func(ap netip.AddrPort) {
		if !ap.IsValid() {
			return
		}
		if _, ok := seen[ap]; ok {
			return
		}
		seen[ap] = struct{}{}
		dests = append(dests, ap)
	}

	if conn != nil {
		remote := addrPortOf(socksaddrFromNet(conn.RemoteAddr()))
		local := addrPortOf(socksaddrFromNet(conn.LocalAddr()))
		if remote.IsValid() {
			source = remote
		}
		add(local)
	}
	if !source.IsValid() && metadata.Source.IsValid() {
		source = metadata.Source.AddrPort()
	}
	if metadata.OriginDestination.IsValid() && metadata.OriginDestination.IsIP() {
		add(metadata.OriginDestination.AddrPort())
	}
	if metadata.Destination.IsIP() {
		add(metadata.Destination.AddrPort())
	}
	port := metadata.OriginDestination.Port
	if port == 0 {
		port = metadata.Destination.Port
	}
	if port != 0 {
		for _, addr := range metadata.DestinationAddresses {
			if addr.IsValid() {
				add(netip.AddrPortFrom(addr, port))
			}
		}
	}
	// Local-port-only fallback: TCP source port is unique on the host
	// (requires throng TCP local-port fallback patch).
	dests = append(dests, netip.AddrPort{})
	return source, dests
}

func lookupProcessOwner(network string, source netip.AddrPort, dests []netip.AddrPort) *adapter.ConnectionOwner {
	searcher := getProcessSearcher()
	if searcher == nil || !source.IsValid() {
		return nil
	}
	if network == "" {
		network = N.NetworkTCP
	}
	var best *adapter.ConnectionOwner
	for _, dest := range dests {
		info, err := sboxprocess.FindProcessInfo(searcher, context.Background(), network, source, dest)
		if err != nil || !processOwnerKnown(info) {
			continue
		}
		info = enrichOwnerPath(info)
		best = ownerRicher(best, info)
		// Full hit: path+pid — stop early.
		if best != nil && best.ProcessPath != "" && best.ProcessID > 0 {
			return best
		}
	}
	return best
}

func attachProcessToTrackers(box *boxbox.Box, source netip.AddrPort, info *adapter.ConnectionOwner) {
	info = enrichOwnerPath(info)
	if !processOwnerKnown(info) || box == nil || !source.IsValid() {
		return
	}
	clashServer := service.FromContext[adapter.ClashServer](box.Context())
	if clashServer == nil {
		return
	}
	clash, ok := clashServer.(*clashapi.Server)
	if !ok {
		return
	}
	for _, c := range clash.TrafficManager().Connections() {
		if !c.Metadata.Source.IsValid() {
			continue
		}
		if c.Metadata.Source.AddrPort() != source {
			continue
		}
		// Prefer richer owner (path > pid-only); upgrade empty / pid-only trackers.
		if trackerHasOwner(c) {
			merged := ownerRicher(c.Metadata.ProcessInfo, info)
			if merged == c.Metadata.ProcessInfo {
				// still try path fill on existing pid-only
				if filled := enrichOwnerPath(c.Metadata.ProcessInfo); filled != nil && filled.ProcessPath != "" && c.Metadata.ProcessInfo.ProcessPath == "" {
					c.Metadata.ProcessInfo = filled
				}
				continue
			}
			c.Metadata.ProcessInfo = merged
			continue
		}
		c.Metadata.ProcessInfo = info
	}
}

// liveBoxSnapshot returns the current box pointer under lifeMu (may be nil).
func liveBoxSnapshot() *boxbox.Box {
	lifeMu.RLock()
	box := boxInstance
	lifeMu.RUnlock()
	return box
}

// resolveOwnerOnce does a single multi-candidate lookup and attaches the result.
// Returns true when owner is known (path and/or pid).
func resolveOwnerOnce(conn net.Conn, metadata adapter.InboundContext) bool {
	box := liveBoxSnapshot()
	if box == nil {
		return false
	}
	if processOwnerKnown(metadata.ProcessInfo) {
		// Already known from matchRule / route find_process; mirror + path fill.
		if metadata.Source.IsValid() {
			attachProcessToTrackers(box, metadata.Source.AddrPort(), metadata.ProcessInfo)
		}
		// true only if we now have path (or at least pid). path fill may still need retry.
		info := enrichOwnerPath(metadata.ProcessInfo)
		return info != nil && info.ProcessPath != ""
	}
	source, dests := exactSocketTuple(conn, metadata)
	info := lookupProcessOwner(metadata.Network, source, dests)
	if info == nil {
		return false
	}
	attachProcessToTrackers(box, source, info)
	return info.ProcessPath != ""
}

// scheduleOwnerRetries: PCB often lags route by tens of ms; path fill may also
// need a moment after pid is known. Bounded — not a poll loop.
func scheduleOwnerRetries(conn net.Conn, metadata adapter.InboundContext) {
	box := liveBoxSnapshot()
	source, dests := exactSocketTuple(conn, metadata)
	network := metadata.Network
	if network == "" {
		network = N.NetworkTCP
	}
	// 50 / 150 / 400 ms — cover late PCB + path fill under load
	delays := []time.Duration{50 * time.Millisecond, 150 * time.Millisecond, 400 * time.Millisecond}
	for _, d := range delays {
		delay := d
		time.AfterFunc(delay, func() {
			if box == nil || liveBoxSnapshot() != box {
				return
			}
			// Re-check trackers for this source; skip if already path+pid.
			if clashServer := service.FromContext[adapter.ClashServer](box.Context()); clashServer != nil {
				if clash, ok := clashServer.(*clashapi.Server); ok {
					full := true
					any := false
					for _, c := range clash.TrafficManager().Connections() {
						if !c.Metadata.Source.IsValid() || c.Metadata.Source.AddrPort() != source {
							continue
						}
						any = true
						info := c.Metadata.ProcessInfo
						if info == nil || info.ProcessPath == "" || info.ProcessID == 0 {
							full = false
							// try path fill on pid-only without re-search
							if info != nil && info.ProcessID > 0 && info.ProcessPath == "" {
								if filled := enrichOwnerPath(info); filled != nil && filled.ProcessPath != "" {
									c.Metadata.ProcessInfo = filled
								}
							}
						}
					}
					if any && full {
						return
					}
				}
			}
			if info := lookupProcessOwner(network, source, dests); info != nil {
				attachProcessToTrackers(box, source, info)
			}
		})
	}
}

func (processOwnerEnricher) RoutedConnection(ctx context.Context, conn net.Conn, metadata adapter.InboundContext, matchedRule adapter.Rule, matchOutbound adapter.Outbound) net.Conn {
	// Clash tracker already joined this conn (registered before us).
	if !resolveOwnerOnce(conn, metadata) {
		scheduleOwnerRetries(conn, metadata)
	} else {
		// Have path; still schedule one cheap path-fill upgrade if pid-only somehow.
		// resolveOwnerOnce true means path present — no retry needed.
	}
	// Even when resolve succeeds with path, ensure pid is attached if route had only path.
	// (throng usually has both after ProcessID patch.)
	return conn
}

func (processOwnerEnricher) RoutedPacketConnection(ctx context.Context, conn N.PacketConn, metadata adapter.InboundContext, matchedRule adapter.Rule, matchOutbound adapter.Outbound) N.PacketConn {
	if !resolveOwnerOnce(nil, metadata) {
		scheduleOwnerRetries(nil, metadata)
	}
	return conn
}
