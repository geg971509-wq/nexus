package test_utils

import (
	"NexusCore/internal/boxbox"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing/common/metadata"
	"github.com/sagernet/sing/service"
	"io"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

type IPInfo struct {
	// Matches runtime-stats / profile-start lookup (ip-api.com).
	// json "query" is the observed egress IP; keep IP for callers.
	Status      string `json:"status"`
	IP          string `json:"query"`
	CountryCode string `json:"countryCode"`
}

var IPReporter IPTestReporter

const IPTestTimeout = 5 * time.Second

// Free ip-api is fragile under high parallel load; URL-test concurrency is too high here.
const MaxConcurrentIPTests = 8

// Same provider as DialogRuntimeStats / profile-start egress snapshot so
// "Resolve Selected Out IP" agrees with the runtime stats Out IP (typically IPv4).
const ipInfoAPI = "http://ip-api.com/json/?fields=status,message,query,countryCode"

type IPTestResult struct {
	Result IPInfo
	Tag    string
	Error  error
}

type IPTestReporter struct {
	results []*IPTestResult
	mu      sync.Mutex
}

func (u *IPTestReporter) AddResult(result *IPTestResult) {
	u.mu.Lock()
	defer u.mu.Unlock()
	u.results = append(u.results, result)
}

func (u *IPTestReporter) Results() []*IPTestResult {
	u.mu.Lock()
	defer u.mu.Unlock()
	res := u.results
	u.results = nil
	return res
}

func BatchIPTest(ctx context.Context, i *boxbox.Box, outboundTags []string, maxConcurrency int, timeout time.Duration) []*IPTestResult {
	if timeout <= 0 {
		timeout = IPTestTimeout
	}
	if maxConcurrency <= 0 || maxConcurrency > MaxConcurrentIPTests {
		maxConcurrency = MaxConcurrentIPTests
	}
	outbounds := service.FromContext[adapter.OutboundManager](i.Context())
	resMap := make(map[string]*IPTestResult)
	resAccess := sync.Mutex{}
	limiter := make(chan struct{}, maxConcurrency)

	wg := &sync.WaitGroup{}
	for _, tag := range outboundTags {
		select {
		case <-ctx.Done():
			resAccess.Lock()
			resMap[tag] = &IPTestResult{
				Tag:   tag,
				Error: errors.New("test aborted"),
			}
			resAccess.Unlock()
			continue
		default:
		}

		outbound, found := outbounds.Outbound(tag)
		if !found {
			u := &IPTestResult{Tag: tag, Error: fmt.Errorf("no outbound with tag %s found", tag)}
			resAccess.Lock()
			resMap[tag] = u
			resAccess.Unlock()
			IPReporter.AddResult(u)
			continue
		}

		time.Sleep(2 * time.Millisecond) // don't spawn goroutines too quickly
		select {
		case limiter <- struct{}{}:
		case <-ctx.Done():
			resAccess.Lock()
			resMap[tag] = &IPTestResult{Tag: tag, Error: errors.New("test aborted")}
			resAccess.Unlock()
			continue
		}
		wg.Add(1)
		go func(t string, outbound adapter.Outbound) {
			defer wg.Done()
			defer func() { <-limiter }()
			client := &http.Client{
				Transport: &http.Transport{
					// Prefer IPv4 so Out IP matches runtime-stats.
					DialContext: func(_ context.Context, network string, addr string) (net.Conn, error) {
						return outbound.DialContext(ctx, "tcp4", metadata.ParseSocksaddr(addr))
					},
					DisableKeepAlives: true,
				},
				Timeout: timeout,
			}
			resp, err := ipTestWithRetry(ctx, client)
			resAccess.Lock()
			u := &IPTestResult{
				Result: resp,
				Tag:    t,
				Error:  err,
			}
			resMap[t] = u
			IPReporter.AddResult(u)
			resAccess.Unlock()
		}(tag, outbound)
	}

	wg.Wait()
	res := make([]*IPTestResult, 0, len(outboundTags))
	for _, tag := range outboundTags {
		res = append(res, resMap[tag])
	}

	return res
}

func ipTestWithRetry(ctx context.Context, client *http.Client) (IPInfo, error) {
	var last IPInfo
	var lastErr error
	for attempt := 0; attempt < 2; attempt++ {
		if attempt > 0 {
			select {
			case <-ctx.Done():
				return last, ctx.Err()
			case <-time.After(250 * time.Millisecond):
			}
		}
		last, lastErr = ipTest(ctx, client)
		if lastErr == nil {
			return last, nil
		}
		// Don't retry aborts.
		if errors.Is(lastErr, context.Canceled) || errors.Is(lastErr, context.DeadlineExceeded) {
			return last, lastErr
		}
	}
	return last, lastErr
}

func ipTest(ctx context.Context, client *http.Client) (IPInfo, error) {
	var res IPInfo
	req, err := http.NewRequestWithContext(ctx, "GET", ipInfoAPI, nil)
	if err != nil {
		return res, err
	}
	req.Header.Set("Accept", "application/json")
	req.Header.Set("User-Agent", "upstream/IPTest")
	resp, err := client.Do(req)
	if err != nil {
		return res, err
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return res, err
	}
	trim := strings.TrimSpace(string(data))
	if trim == "" {
		return res, errors.New("ip-api empty body")
	}
	if trim[0] == '<' || resp.StatusCode >= 400 {
		// Rate-limit / WAF / proxy interstitial HTML.
		snippet := trim
		if len(snippet) > 80 {
			snippet = snippet[:80] + "..."
		}
		return res, errors.New("ip-api non-json response (HTTP " + resp.Status + "): " + snippet)
	}
	err = json.Unmarshal(data, &res)
	if err != nil {
		return res, err
	}
	if res.Status != "" && res.Status != "success" {
		return res, errors.New("ip-api status: " + res.Status)
	}
	if res.IP == "" {
		return res, errors.New("ip-api returned empty query")
	}
	return res, nil
}
