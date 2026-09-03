package test_utils

import (
	"NexusCore/internal/boxbox"
	"context"
	"errors"
	"fmt"
	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing/common/metadata"
	"github.com/sagernet/sing/service"
	"net"
	"net/http"
	"sync"
	"time"
)

var URLReporter URLTestReporter

const URLTestTimeout = 3 * time.Second

type URLTestResult struct {
	Duration time.Duration
	Tag      string
	Error    error
}

type URLTestReporter struct {
	results []*URLTestResult
	mu      sync.Mutex
}

func (u *URLTestReporter) AddResult(result *URLTestResult) {
	u.mu.Lock()
	defer u.mu.Unlock()
	u.results = append(u.results, result)
}

func (u *URLTestReporter) Results() []*URLTestResult {
	u.mu.Lock()
	defer u.mu.Unlock()
	res := u.results
	u.results = nil
	return res
}

func BatchURLTest(ctx context.Context, i *boxbox.Box, outboundTags []string, url string, maxConcurrency int, twice bool, timeout time.Duration) []*URLTestResult {
	if timeout <= 0 {
		timeout = URLTestTimeout
	}
	if maxConcurrency <= 0 {
		maxConcurrency = MaxConcurrentTests
	}
	outbounds := service.FromContext[adapter.OutboundManager](i.Context())
	resMap := make(map[string]*URLTestResult)
	resAccess := sync.Mutex{}
	limiter := make(chan struct{}, maxConcurrency)

	wg := &sync.WaitGroup{}
	for _, tag := range outboundTags {
		select {
		case <-ctx.Done():
			resAccess.Lock()
			resMap[tag] = &URLTestResult{
				Duration: 0,
				Tag:      tag,
				Error:    errors.New("test aborted"),
			}
			resAccess.Unlock()
			continue
		default:
		}

		outbound, found := outbounds.Outbound(tag)
		if !found {
			u := &URLTestResult{Tag: tag, Error: fmt.Errorf("no outbound with tag %s found", tag)}
			resAccess.Lock()
			resMap[tag] = u
			resAccess.Unlock()
			URLReporter.AddResult(u)
			continue
		}

		time.Sleep(2 * time.Millisecond) // don't spawn goroutines too quickly
		select {
		case limiter <- struct{}{}:
		case <-ctx.Done():
			resAccess.Lock()
			resMap[tag] = &URLTestResult{Tag: tag, Error: errors.New("test aborted")}
			resAccess.Unlock()
			continue
		}
		wg.Add(1)
		go func(t string, outbound adapter.Outbound) {
			defer wg.Done()
			defer func() { <-limiter }()
			client := &http.Client{
				Transport: &http.Transport{
					DialContext: func(_ context.Context, network string, addr string) (net.Conn, error) {
						return outbound.DialContext(ctx, "tcp", metadata.ParseSocksaddr(addr))
					},
				},
				Timeout: timeout,
			}
			// to properly measure muxed configs, let's do the test twice
			duration, err := urlTest(ctx, client, url)
			if err == nil && twice {
				duration, err = urlTest(ctx, client, url)
			}
			resAccess.Lock()
			u := &URLTestResult{
				Duration: duration,
				Tag:      t,
				Error:    err,
			}
			resMap[t] = u
			URLReporter.AddResult(u)
			resAccess.Unlock()
		}(tag, outbound)
	}

	wg.Wait()
	res := make([]*URLTestResult, 0, len(outboundTags))
	for _, tag := range outboundTags {
		res = append(res, resMap[tag])
	}

	return res
}

func urlTest(ctx context.Context, client *http.Client, url string) (time.Duration, error) {
	begin := time.Now()
	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return 0, err
	}
	resp, err := client.Do(req)
	if err != nil {
		return 0, err
	}
	_ = resp.Body.Close()
	return time.Since(begin), nil
}
