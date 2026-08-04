package main

import (
	"context"
	"net"
	"net/netip"
	"sync"
	"time"

	"github.com/sagernet/sing-box/adapter"
	sboxprocess "github.com/sagernet/sing-box/common/process"
	"github.com/sagernet/sing-box/experimental/clashapi"
	"github.com/sagernet/sing-box/experimental/clashapi/trafficontrol"
	M "github.com/sagernet/sing/common/metadata"
	N "github.com/sagernet/sing/common/network"
	"github.com/sagernet/sing/service"
)

// processOwnerEnricher runs once per routed connection, right after clash
// traffic tracking joins the conn. It uses the live socket 4-tuple (most exact
// source available on mixed/system-proxy) plus metadata destinations (TUN),
// writes ProcessPath into the tracker, and never starts a poll loop.
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
	// Local-port-only fallback: TCP source port is unique on the host.
	dests = append(dests, netip.AddrPort{})
	return source, dests
}

func lookupProcessPath(network string, source netip.AddrPort, dests []netip.AddrPort) *adapter.ConnectionOwner {
	searcher := getProcessSearcher()
	if searcher == nil || !source.IsValid() {
		return nil
	}
	if network == "" {
		network = N.NetworkTCP
	}
	for _, dest := range dests {
		info, err := sboxprocess.FindProcessInfo(searcher, context.Background(), network, source, dest)
		if err != nil || info == nil || info.ProcessPath == "" {
			continue
		}
		return info
	}
	return nil
}

func attachProcessToTrackers(source netip.AddrPort, info *adapter.ConnectionOwner) {
	if info == nil || info.ProcessPath == "" || boxInstance == nil || !source.IsValid() {
		return
	}
	clashServer := service.FromContext[adapter.ClashServer](boxInstance.Context())
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
		if processPathOf(c) != "" {
			continue
		}
		c.Metadata.ProcessInfo = info
	}
}

// resolveOwnerOnce does a single multi-candidate lookup and attaches the result.
// Returns true when a process path was written.
func resolveOwnerOnce(conn net.Conn, metadata adapter.InboundContext) bool {
	if metadata.ProcessInfo != nil && metadata.ProcessInfo.ProcessPath != "" {
		// Already known from matchRule; still mirror onto tracker if needed.
		if metadata.Source.IsValid() {
			attachProcessToTrackers(metadata.Source.AddrPort(), metadata.ProcessInfo)
		}
		return true
	}
	source, dests := exactSocketTuple(conn, metadata)
	info := lookupProcessPath(metadata.Network, source, dests)
	if info == nil {
		return false
	}
	attachProcessToTrackers(source, info)
	return true
}

func (processOwnerEnricher) RoutedConnection(ctx context.Context, conn net.Conn, metadata adapter.InboundContext, matchedRule adapter.Rule, matchOutbound adapter.Outbound) net.Conn {
	// Clash tracker already joined this conn (registered before us). One exact
	// lookup now — no QueryConnections poll loop.
	if resolveOwnerOnce(conn, metadata) {
		return conn
	}
	// Single deferred attempt only if the PCB entry was not ready at route time.
	// Not a poll: at most one extra lookup ~100ms later.
	source, dests := exactSocketTuple(conn, metadata)
	network := metadata.Network
	time.AfterFunc(100*time.Millisecond, func() {
		if info := lookupProcessPath(network, source, dests); info != nil {
			attachProcessToTrackers(source, info)
		}
	})
	return conn
}

func (processOwnerEnricher) RoutedPacketConnection(ctx context.Context, conn N.PacketConn, metadata adapter.InboundContext, matchedRule adapter.Rule, matchOutbound adapter.Outbound) N.PacketConn {
	// PacketConn may not expose a stable net.Conn; metadata endpoints only.
	if resolveOwnerOnce(nil, metadata) {
		return conn
	}
	source, dests := exactSocketTuple(nil, metadata)
	network := metadata.Network
	if network == "" {
		network = N.NetworkUDP
	}
	time.AfterFunc(100*time.Millisecond, func() {
		if info := lookupProcessPath(network, source, dests); info != nil {
			attachProcessToTrackers(source, info)
		}
	})
	return conn
}
