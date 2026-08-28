//go:build darwin && arm64

package main

import (
	"context"
	"errors"
	"fmt"
	"log"
	"net/netip"
	"os"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"NexusCore/gen"
	"NexusCore/internal/boxbox"
	"NexusCore/internal/boxmain"
	"NexusCore/internal/process"
	"NexusCore/internal/sys"
	"NexusCore/internal/wg"
	"NexusCore/test_utils"

	"github.com/google/shlex"
	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing-box/experimental/clashapi"
	"github.com/sagernet/sing-box/experimental/clashapi/trafficontrol"
	E "github.com/sagernet/sing/common/exceptions"
	"github.com/sagernet/sing/service"
)

var (
	boxInstance     *boxbox.Box
	extraProcess    *process.Process
	needUnsetDNS    bool
	instanceCancel  context.CancelFunc
	debug           bool
	activeProfileID atomic.Int32
	// lifeMu serializes Start/Stop and short Query* bodies.
	// boxPins tracks long TestCurrent/SpeedTest users of the live box after RLock drop.
	// ponytail: global lock; finer per-subsystem locks if Start latency matters.
	lifeMu  sync.RWMutex
	boxPins atomic.Int32
)

// pinBox snapshots the live box under RLock and pins it so Stop waits before Close.
// Caller must invoke the release func (idempotent not required — call once).
func pinBox() (*boxbox.Box, func()) {
	lifeMu.RLock()
	b := boxInstance
	if b == nil {
		lifeMu.RUnlock()
		return nil, func() {}
	}
	boxPins.Add(1)
	lifeMu.RUnlock()
	var once sync.Once
	return b, func() {
		once.Do(func() { boxPins.Add(-1) })
	}
}

// cleanupAll tears down box + extra process + DNS. Idempotent; call only while
// holding lifeMu write lock (or from a path that already owns exclusive lifecycle).
func cleanupAll() {
	// Wait for TestCurrent/SpeedTest pins so Close does not race traffic/test paths.
	deadline := time.Now().Add(2 * time.Second)
	for boxPins.Load() > 0 && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	if needUnsetDNS {
		needUnsetDNS = false
		if boxInstance != nil {
			if err := sys.SetSystemDNS("Empty", boxInstance.Network().InterfaceMonitor()); err != nil {
				log.Println("Failed to unset system DNS:", err)
			}
		}
	}
	if boxInstance != nil {
		boxInstance.CloseWithTimeout(instanceCancel, time.Second*2, log.Println, true)
		boxInstance = nil
		instanceCancel = nil
	} else if instanceCancel != nil {
		instanceCancel()
		instanceCancel = nil
	}
	if extraProcess != nil {
		extraProcess.Stop()
		extraProcess = nil
	}
	activeProfileID.Store(-1)
}

func init() {
	activeProfileID.Store(-1)
}

type server struct {
	gen.UnimplementedLibcoreServiceServer
}

// To returns a pointer to the given value.
func To[T any](v T) *T {
	return &v
}

func (s *server) Start(ctx context.Context, in *gen.LoadConfigReq) (out *gen.ErrorResp, _ error) {
	var err error
	// skipCleanup: "already started" must not tear down the live tunnel.
	skipCleanup := false

	lifeMu.Lock()
	defer lifeMu.Unlock()

	defer func() {
		out = &gen.ErrorResp{}
		if err != nil {
			out.Error = To(err.Error())
			if !skipCleanup {
				// Single cleanup for every partial Start failure (extra/box/DNS).
				cleanupAll()
			}
		}
	}()

	if debug {
		log.Println("Start:", *in.CoreConfig)
	}

	if boxInstance != nil {
		err = errors.New("instance already started")
		skipCleanup = true
		return
	}

	if *in.NeedExtraProcess {
		args, e := shlex.Split(in.GetExtraProcessArgs())
		if e != nil {
			err = E.Cause(e, "Failed to parse args")
			return
		}
		var extraConfPath, extraCleanupPath string
		if in.ExtraProcessConf != nil {
			// The Core (not the GUI) creates the config, in a fresh randomly
			// named temp file that cannot be hijacked by symlink/pre-existing
			// file tricks even when the Core is elevated. See CreateExtraConfig.
			extraConfPath, extraCleanupPath, e = process.CreateExtraConfig(*in.ExtraProcessConf)
			if e != nil {
				err = E.Cause(e, "Failed to create extra.conf")
				return
			}
			for idx, arg := range args {
				if strings.Contains(arg, "%s") {
					args[idx] = fmt.Sprintf(arg, extraConfPath)
					break
				}
			}
		}

		extraProcess = process.NewProcess(*in.ExtraProcessPath, args, *in.ExtraNoOut)
		extraProcess.SetCleanupPath(extraCleanupPath)
		err = extraProcess.Start()
		if err != nil {
			return
		}
	}

	boxInstance, instanceCancel, err = boxmain.Create([]byte(*in.CoreConfig))
	if err != nil {
		return
	}
	// After clash tracker: one exact process lookup per routed connection.
	boxInstance.Router().AppendTracker(processOwnerEnricher{})

	if in.GetTunIpv4Cidr() != "" {
		tunCIDR := in.GetTunIpv4Cidr()
		tunPrefix, parseErr := netip.ParsePrefix(tunCIDR)
		if parseErr != nil || !tunPrefix.Addr().Is4() {
			err = fmt.Errorf("invalid tun_ipv4_cidr %q", tunCIDR)
			return
		}

		tunDNS := tunPrefix.Addr().Next()
		if !tunDNS.IsValid() || !tunDNS.Is4() {
			err = fmt.Errorf("got invalid DNS IP from tun_ipv4_cidr: %s", tunDNS)
			return
		}

		if e := sys.SetSystemDNS(tunDNS.String(), boxInstance.Network().InterfaceMonitor()); e != nil {
			log.Println("Failed to set system DNS:", e)
		}

		needUnsetDNS = true
	}
	activeProfileID.Store(in.GetProfileId())

	return
}

func (s *server) Stop(ctx context.Context, in *gen.EmptyReq) (out *gen.ErrorResp, _ error) {
	lifeMu.Lock()
	defer lifeMu.Unlock()
	// Always cleanupAll — also clears an orphan extra process when box is nil.
	cleanupAll()
	return &gen.ErrorResp{}, nil
}

func (s *server) QueryState(ctx context.Context, in *gen.EmptyReq) (*gen.CoreStateResponse, error) {
	profileID := activeProfileID.Load()
	return &gen.CoreStateResponse{
		Running:   To(profileID >= 0),
		ProfileId: To(profileID),
	}, nil
}

func (s *server) CheckConfig(ctx context.Context, in *gen.LoadConfigReq) (out *gen.ErrorResp, _ error) {
	out = &gen.ErrorResp{}
	// Recover from panics inside boxmain.Check (e.g. malformed configs that trigger
	// sing-box internal panics). Without this, the panic propagates to main() which
	// calls os.Exit(0) and kills the entire core process. The full goroutine stack
	// goes to the operator log; the wire response carries only the panic value.
	defer func() {
		if r := recover(); r != nil {
			buf := make([]byte, 4096)
			n := runtime.Stack(buf, false)
			log.Printf("CheckConfig panic: %v\n%s", r, buf[:n])
			out.Error = To(fmt.Sprintf("CheckConfig panic: %v", r))
		}
	}()
	err := boxmain.Check([]byte(*in.CoreConfig))
	if err != nil {
		out.Error = To(err.Error())
	}
	return
}

func (s *server) Test(ctx context.Context, in *gen.TestReq) (*gen.TestResp, error) {
	var testInstance *boxbox.Box
	var cancel context.CancelFunc
	var err error
	twice := true
	if *in.TestCurrent {
		var release func()
		testInstance, release = pinBox()
		if testInstance == nil {
			return &gen.TestResp{Results: []*gen.URLTestResp{{
				OutboundTag: To("proxy"),
				LatencyMs:   To(int32(0)),
				Error:       To("Instance is not running"),
			}}}, nil
		}
		defer release()
		twice = false
	} else {
		testInstance, cancel, err = boxmain.Create([]byte(*in.Config))
		if err != nil {
			return nil, err
		}
		defer testInstance.CloseWithTimeout(cancel, 2*time.Second, log.Println, false)
	}

	needDefault := false
	outboundTags := in.OutboundTags
	if *in.TestCurrent {
		_, exists := testInstance.Outbound().Outbound("proxy")
		if !exists {
			needDefault = true
		} else {
			outboundTags = []string{"proxy"}
		}
	}
	if *in.UseDefaultOutbound || needDefault {
		outbound := testInstance.Outbound().Default()
		outboundTags = []string{outbound.Tag()}
	}

	maxConcurrency := *in.MaxConcurrency
	if maxConcurrency >= 500 || maxConcurrency == 0 {
		maxConcurrency = test_utils.MaxConcurrentTests
	}
	results := test_utils.BatchURLTest(test_utils.TestCtx, testInstance, outboundTags, *in.Url, int(maxConcurrency), twice, time.Duration(*in.TestTimeoutMs)*time.Millisecond)

	res := make([]*gen.URLTestResp, 0)
	for idx, data := range results {
		errStr := ""
		if data.Error != nil {
			errStr = data.Error.Error()
		}
		res = append(res, &gen.URLTestResp{
			OutboundTag: To(outboundTags[idx]),
			LatencyMs:   To(int32(data.Duration.Milliseconds())),
			Error:       To(errStr),
		})
	}

	return &gen.TestResp{Results: res}, nil
}

func (s *server) StopTest(ctx context.Context, in *gen.EmptyReq) (*gen.EmptyResp, error) {
	test_utils.CancelTests()
	test_utils.TestCtx, test_utils.CancelTests = context.WithCancel(context.Background())

	return &gen.EmptyResp{}, nil
}

func (s *server) QueryURLTest(ctx context.Context, in *gen.EmptyReq) (out *gen.QueryURLTestResponse, _ error) {
	results := test_utils.URLReporter.Results()
	out = &gen.QueryURLTestResponse{}
	for _, r := range results {
		errStr := ""
		if r.Error != nil {
			errStr = r.Error.Error()
		}
		out.Results = append(out.Results, &gen.URLTestResp{
			OutboundTag: To(r.Tag),
			LatencyMs:   To(int32(r.Duration.Milliseconds())),
			Error:       To(errStr),
		})
	}
	return
}

func (s *server) IPTest(ctx context.Context, in *gen.IPTestRequest) (*gen.IPTestResp, error) {
	var testInstance *boxbox.Box
	var cancel context.CancelFunc
	var err error
	testInstance, cancel, err = boxmain.Create([]byte(*in.Config))
	if err != nil {
		return nil, err
	}
	defer testInstance.CloseWithTimeout(cancel, 2*time.Second, log.Println, false)

	outboundTags := in.OutboundTags
	if *in.UseDefaultOutbound {
		outbound := testInstance.Outbound().Default()
		outboundTags = []string{outbound.Tag()}
	}

	maxConcurrency := *in.MaxConcurrency
	if maxConcurrency >= 500 || maxConcurrency == 0 {
		maxConcurrency = test_utils.MaxConcurrentTests
	}
	timeout := time.Duration(*in.TestTimeoutMs) * time.Millisecond
	results := test_utils.BatchIPTest(test_utils.TestCtx, testInstance, outboundTags, int(maxConcurrency), timeout)

	res := make([]*gen.IPTestRes, 0, len(results))
	for idx, data := range results {
		errStr := ""
		if data.Error != nil {
			errStr = data.Error.Error()
		}
		tag := outboundTags[idx]
		res = append(res, &gen.IPTestRes{
			OutboundTag: To(tag),
			Ip:          To(data.Result.IP),
			CountryCode: To(data.Result.CountryCode),
			Error:       To(errStr),
		})
	}
	return &gen.IPTestResp{Results: res}, nil
}

func (s *server) QueryIPTest(ctx context.Context, in *gen.EmptyReq) (out *gen.QueryIPTestResponse, _ error) {
	results := test_utils.IPReporter.Results()
	out = &gen.QueryIPTestResponse{}
	for _, r := range results {
		errStr := ""
		if r.Error != nil {
			errStr = r.Error.Error()
		}
		out.Results = append(out.Results, &gen.IPTestRes{
			OutboundTag: To(r.Tag),
			Ip:          To(r.Result.IP),
			CountryCode: To(r.Result.CountryCode),
			Error:       To(errStr),
		})
	}
	return
}

func (s *server) QueryStats(ctx context.Context, in *gen.EmptyReq) (out *gen.QueryStatsResp, err error) {
	out = &gen.QueryStatsResp{}
	out.Ups = make(map[string]int64)
	out.Downs = make(map[string]int64)
	// Hold RLock for the whole short body so Stop cannot Close under our feet.
	lifeMu.RLock()
	defer lifeMu.RUnlock()
	box := boxInstance
	if box == nil {
		return
	}
	clash := service.FromContext[adapter.ClashServer](box.Context())
	if clash == nil {
		return
	}
	cApi, ok := clash.(*clashapi.Server)
	if !ok {
		log.Println("Failed to assert clash server")
		err = E.New("invalid clash server type")
		return
	}
	outbounds := service.FromContext[adapter.OutboundManager](box.Context())
	if outbounds == nil {
		log.Println("Failed to get outbound manager")
		err = E.New("nil outbound manager")
		return
	}
	endpoints := service.FromContext[adapter.EndpointManager](box.Context())
	if endpoints == nil {
		log.Println("Failed to get endpoint manager")
		err = E.New("nil endpoint manager")
		return
	}
	for _, ob := range outbounds.Outbounds() {
		u, d := cApi.TrafficManager().TotalOutbound(ob.Tag())
		out.Ups[ob.Tag()] = u
		out.Downs[ob.Tag()] = d
	}
	for _, ep := range endpoints.Endpoints() {
		u, d := cApi.TrafficManager().TotalOutbound(ep.Tag())
		out.Ups[ep.Tag()] = u
		out.Downs[ep.Tag()] = d
	}
	return
}

// connMetaToProto maps one tracker's metadata into the wire type. Shared by the
// active and closed lists so both carry identical, enriched fields.
// Process path/pid filled by route find_process + processOwnerEnricher.
func connMetaToProto(c *trafficontrol.TrackerMetadata) *gen.ConnectionMetaData {
	processName := ""
	// ProcessInfo is shared state the enricher's timers also write, and the
	// persist below makes this a writer too — two concurrent polls race without
	// the lock, never mind the timers. See ownerMu in process_owner.go.
	ownerMu.Lock()
	processPath := processPathOf(c)
	var processID uint32
	if c != nil && c.Metadata.ProcessInfo != nil {
		processID = c.Metadata.ProcessInfo.ProcessID
	}
	// Query-time path fill under setuid Core: throng may leave path empty even with pid.
	if processPath == "" && processID > 0 {
		if filled := processPathFromPID(processID); filled != "" {
			processPath = filled
			// persist onto tracker so later polls skip re-resolve
			if c != nil && c.Metadata.ProcessInfo != nil {
				c.Metadata.ProcessInfo.ProcessPath = filled
			}
		}
	}
	ownerMu.Unlock()
	if processPath != "" {
		spl := strings.Split(processPath, string(os.PathSeparator))
		processName = spl[len(spl)-1]
	} else if processID > 0 {
		// path blocked (SIP); still show a stable process label
		processName = fmt.Sprintf("pid %d", processID)
	}
	var closedAt int64
	if !c.ClosedAt.IsZero() {
		closedAt = c.ClosedAt.UnixMilli()
	}
	return &gen.ConnectionMetaData{
		Id:          To(c.ID.String()),
		CreatedAt:   To(c.CreatedAt.UnixMilli()),
		Upload:      To(c.Upload.Load()),
		Download:    To(c.Download.Load()),
		Outbound:    To(c.Outbound),
		Network:     To(c.Metadata.Network),
		Dest:        To(c.Metadata.Destination.String()),
		Protocol:    To(c.Metadata.Protocol),
		Domain:      To(c.Metadata.Domain),
		Process:     To(processName),
		ProcessPath: To(processPath),
		Chain:       c.Chain,
		ClosedAt:    To(closedAt),
		ProcessId:   To(processID),
	}
}

// QueryConnections returns both live connections (for the connection table) and
// the recently-closed ring (so traffic accounting doesn't lose the tail of a
// connection that closed between polls). Process ownership is resolved at route
// time (processOwnerEnricher); this RPC only reports stored fields.
func (s *server) QueryConnections(ctx context.Context, in *gen.EmptyReq) (*gen.QueryConnectionsResp, error) {
	// Hold RLock for the whole short body so Stop cannot Close under our feet.
	lifeMu.RLock()
	defer lifeMu.RUnlock()
	box := boxInstance
	if box == nil {
		return &gen.QueryConnectionsResp{}, nil
	}
	clashServer := service.FromContext[adapter.ClashServer](box.Context())
	if clashServer == nil {
		return &gen.QueryConnectionsResp{}, errors.New("no clash server found")
	}
	clash, ok := clashServer.(*clashapi.Server)
	if !ok {
		return &gen.QueryConnectionsResp{}, errors.New("invalid state, should not be here")
	}
	tm := clash.TrafficManager()

	active := make([]*gen.ConnectionMetaData, 0)
	for _, c := range tm.Connections() {
		active = append(active, connMetaToProto(c))
	}
	closed := make([]*gen.ConnectionMetaData, 0)
	for _, c := range tm.ClosedConnections() {
		closed = append(closed, connMetaToProto(c))
	}
	return &gen.QueryConnectionsResp{Active: active, Closed: closed}, nil
}

func (s *server) IsPrivileged(ctx context.Context, _ *gen.EmptyReq) (*gen.IsPrivilegedResponse, error) {
	return &gen.IsPrivilegedResponse{HasPrivilege: To(os.Geteuid() == 0)}, nil
}

func (s *server) SpeedTest(ctx context.Context, in *gen.SpeedTestRequest) (*gen.SpeedTestResponse, error) {
	if !*in.TestDownload && !*in.TestUpload && !*in.SimpleDownload && !*in.OnlyCountry {
		return nil, errors.New("cannot run empty test")
	}
	var testInstance *boxbox.Box
	var cancel context.CancelFunc
	outboundTags := in.OutboundTags
	var err error
	if *in.TestCurrent {
		var release func()
		testInstance, release = pinBox()
		if testInstance == nil {
			return &gen.SpeedTestResponse{Results: []*gen.SpeedTestResult{{
				OutboundTag: To("proxy"),
				Error:       To("Instance is not running"),
			}}}, nil
		}
		defer release()
	} else {
		testInstance, cancel, err = boxmain.Create([]byte(*in.Config))
		if err != nil {
			return nil, err
		}
		defer cancel()
		defer testInstance.Close()
	}

	needDefault := false
	if *in.TestCurrent {
		_, exists := testInstance.Outbound().Outbound("proxy")
		if !exists {
			needDefault = true
		} else {
			outboundTags = []string{"proxy"}
		}
	}
	if *in.UseDefaultOutbound || needDefault {
		outbound := testInstance.Outbound().Default()
		outboundTags = []string{outbound.Tag()}
	}

	results := test_utils.BatchSpeedTest(test_utils.TestCtx, testInstance, outboundTags, *in.TestDownload, *in.TestUpload, *in.SimpleDownload, *in.SimpleDownloadAddr, time.Duration(*in.TimeoutMs)*time.Millisecond, *in.OnlyCountry, *in.CountryConcurrency)

	res := make([]*gen.SpeedTestResult, 0)
	for _, data := range results {
		errStr := ""
		if data.Error != nil {
			errStr = data.Error.Error()
		}
		res = append(res, &gen.SpeedTestResult{
			DlSpeed:       To(data.DlSpeed),
			UlSpeed:       To(data.UlSpeed),
			Latency:       To(data.Latency),
			OutboundTag:   To(data.Tag),
			Error:         To(errStr),
			ServerName:    To(data.ServerName),
			ServerCountry: To(data.ServerCountry),
			Cancelled:     To(data.Cancelled),
			DlBytes:       To(data.DlBytes),
			UlBytes:       To(data.UlBytes),
		})
	}

	return &gen.SpeedTestResponse{Results: res}, nil
}

func (s *server) QuerySpeedTest(context.Context, *gen.EmptyReq) (*gen.QuerySpeedTestResponse, error) {
	res, isRunning := test_utils.SpTQuerier.Result()
	errStr := ""
	if res.Error != nil {
		errStr = res.Error.Error()
	}
	return &gen.QuerySpeedTestResponse{
		Result: &gen.SpeedTestResult{
			DlSpeed:       To(res.DlSpeed),
			UlSpeed:       To(res.UlSpeed),
			Latency:       To(res.Latency),
			OutboundTag:   To(res.Tag),
			Error:         To(errStr),
			ServerName:    To(res.ServerName),
			ServerCountry: To(res.ServerCountry),
			Cancelled:     To(res.Cancelled),
			DlBytes:       To(res.DlBytes),
			UlBytes:       To(res.UlBytes),
		},
		IsRunning: To(isRunning),
	}, nil
}

func (s *server) QueryCountryTest(ctx context.Context, _ *gen.EmptyReq) (out *gen.QueryCountryTestResponse, _ error) {
	results := test_utils.CountryResults.Results()
	out = &gen.QueryCountryTestResponse{}
	for _, res := range results {
		var errStr string
		if res.Error != nil {
			errStr = res.Error.Error()
		}
		out.Results = append(out.Results, &gen.SpeedTestResult{
			DlSpeed:       To(res.DlSpeed),
			UlSpeed:       To(res.UlSpeed),
			Latency:       To(res.Latency),
			OutboundTag:   To(res.Tag),
			Error:         To(errStr),
			ServerName:    To(res.ServerName),
			ServerCountry: To(res.ServerCountry),
			Cancelled:     To(res.Cancelled),
		})
	}
	return
}

func (s *server) GenWgKeyPair(ctx context.Context, _ *gen.EmptyReq) (out *gen.GenWgKeyPairResponse, _ error) {
	var res gen.GenWgKeyPairResponse
	privateKey, err := wg.GeneratePrivateKey()
	if err != nil {
		res.Error = To(err.Error())
		return &res, nil
	}
	res.PrivateKey = To(privateKey.String())
	res.PublicKey = To(privateKey.PublicKey().String())
	return &res, nil
}
