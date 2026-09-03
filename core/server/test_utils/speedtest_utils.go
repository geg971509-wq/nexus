package test_utils

import (
	"NexusCore/internal"
	"NexusCore/internal/boxbox"
	"context"
	"errors"
	"fmt"
	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing/service"
	"io"
	"net"
	"net/http"
	"sync"
	"sync/atomic"
	"time"
)

var SpTQuerier SpeedTestResultQuerier
var CountryResults CountryTestResults

type SpeedTestResult struct {
	Tag           string
	DlSpeed       string
	UlSpeed       string
	Latency       int32
	ServerName    string
	ServerCountry string
	Error         error
	Cancelled     bool
	DlBytes       int64 // total bytes moved by the test, credited to per-config stats
	UlBytes       int64
}

type SpeedTestResultQuerier struct {
	isRunning bool
	current   SpeedTestResult
	mu        sync.RWMutex
}

func (s *SpeedTestResultQuerier) Result() (SpeedTestResult, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.current, s.isRunning
}

func (s *SpeedTestResultQuerier) storeResult(result *SpeedTestResult) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.current = *result
}

func (s *SpeedTestResultQuerier) setIsRunning(isRunning bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.isRunning = isRunning
}

type CountryTestResults struct {
	results []*SpeedTestResult
	mu      sync.Mutex
}

func (c *CountryTestResults) AddResult(result *SpeedTestResult) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.results = append(c.results, result)
}

func (c *CountryTestResults) Results() []*SpeedTestResult {
	c.mu.Lock()
	defer c.mu.Unlock()
	cp := c.results
	c.results = nil
	return cp
}

func countryTest(ctx context.Context, dialer func(ctx context.Context, network string, address string) (net.Conn, error), res *SpeedTestResult) error {
	srv, err := getSpeedtestServer(ctx, dialer)
	if err != nil {
		return err
	}
	res.ServerName = srv.Name
	res.ServerCountry = srv.Country
	res.Latency = int32(srv.Latency.Milliseconds())
	return nil
}

func BatchSpeedTest(ctx context.Context, i *boxbox.Box, outboundTags []string, testDl, testUl bool, simpleDL bool, simpleAddress string, timeout time.Duration, countryOnly bool, countryConcurrency int32) []*SpeedTestResult {
	outbounds := service.FromContext[adapter.OutboundManager](i.Context())
	results := make([]*SpeedTestResult, 0)
	var queuer chan struct{}
	wg := &sync.WaitGroup{}
	if countryOnly {
		if countryConcurrency <= 0 {
			countryConcurrency = 5
		}
		queuer = make(chan struct{}, countryConcurrency)
	}

testLoop:
	for _, tag := range outboundTags {
		select {
		case <-ctx.Done():
			break testLoop
		default:
		}
		outbound, exists := outbounds.Outbound(tag)
		if !exists {
			results = append(results, &SpeedTestResult{
				Tag:   tag,
				Error: fmt.Errorf("no outbound with tag %s found", tag),
			})
			continue
		}
		res := new(SpeedTestResult)
		res.Tag = tag
		results = append(results, res)

		var err error
		if countryOnly {
			queuer <- struct{}{}
			wg.Add(1)
			go func(res *SpeedTestResult, outbound adapter.Outbound) {
				defer func() { <-queuer }()
				defer wg.Done()
				err := countryTest(ctx, getNetDialer(outbound.DialContext), res)
				if err != nil && !errors.Is(err, context.Canceled) {
					res.Error = err
					fmt.Println("Failed to countryTest with err:", err)
				}
				CountryResults.AddResult(res)
			}(res, outbound)
			continue
		}
		if simpleDL {
			err = simpleDownloadTest(ctx, getNetDialer(outbound.DialContext), res, simpleAddress, timeout)
		} else {
			err = speedTestWithDialer(ctx, getNetDialer(outbound.DialContext), res, testDl, testUl, timeout)
		}
		if err != nil && !errors.Is(err, context.Canceled) {
			res.Error = err
			fmt.Println("Failed to speedtest with err:", err)
		}
		if !testDl && !simpleDL {
			res.DlSpeed = ""
		}
		if !testUl {
			res.UlSpeed = ""
		}
	}
	wg.Wait()

	return results
}

func simpleDownloadTest(ctx context.Context, dialer func(ctx context.Context, network string, address string) (net.Conn, error), res *SpeedTestResult, testURL string, timeout time.Duration) error {
	if timeout <= 0 {
		timeout = URLTestTimeout
	}
	client := &http.Client{
		Transport: &http.Transport{
			DialContext: func(ctx context.Context, network string, addr string) (net.Conn, error) {
				return dialer(ctx, network, addr)
			},
		},
		Timeout: timeout,
	}

	res.ServerName = "N/A"
	res.ServerCountry = "N/A"

	req, err := http.NewRequestWithContext(ctx, "GET", testURL, nil)
	if err != nil {
		return err
	}

	done := make(chan error, 1)
	var downloaded atomic.Int64
	var startUnixNano atomic.Int64
	var latencyMillis atomic.Int64

	go func() {
		reqStart := time.Now()
		resp, err := client.Do(req)
		if err != nil {
			done <- err
			return
		}
		defer resp.Body.Close()
		latencyMillis.Store(time.Since(reqStart).Milliseconds())
		startUnixNano.Store(time.Now().UnixNano())
		writer := &atomicByteCounter{bytes: &downloaded}
		_, err = io.Copy(writer, resp.Body)
		done <- err
	}()

	ticker := time.NewTicker(time.Millisecond * 50)
	defer ticker.Stop()

	SpTQuerier.setIsRunning(true)
	defer SpTQuerier.setIsRunning(false)

	publish := func() {
		bytes := downloaded.Load()
		startNanos := startUnixNano.Load()
		if startNanos != 0 {
			res.DlSpeed = internal.BrateToStr(internal.CalculateBRate(float64(bytes), time.Unix(0, startNanos)))
		}
		res.DlBytes = bytes
		res.Latency = int32(latencyMillis.Load())
		SpTQuerier.storeResult(res)
	}

	for {
		select {
		case err := <-done:
			publish()
			return err
		case <-ctx.Done():
			res.Cancelled = true
			publish()
			return ctx.Err()
		case <-ticker.C:
			publish()
		}
	}
}

type atomicByteCounter struct {
	bytes *atomic.Int64
}

func (w *atomicByteCounter) Write(p []byte) (int, error) {
	w.bytes.Add(int64(len(p)))
	return len(p), nil
}

func speedTestWithDialer(ctx context.Context, dialer func(ctx context.Context, network string, address string) (net.Conn, error), res *SpeedTestResult, testDl, testUl bool, timeout time.Duration) error {
	srv, err := getSpeedtestServer(ctx, dialer)
	if err != nil {
		return err
	}
	res.ServerName = srv.Name
	res.ServerCountry = srv.Country

	SpTQuerier.setIsRunning(true)
	defer SpTQuerier.setIsRunning(false)
	SpTQuerier.storeResult(res)

	if testDl {
		timeoutCtx, cancel := context.WithTimeout(ctx, timeout)
		err = srv.DownloadTestContext(timeoutCtx)
		cancel()
		if err != nil {
			if errors.Is(err, context.Canceled) {
				res.Cancelled = true
			}
			return err
		}
	}
	if testUl {
		timeoutCtx, cancel := context.WithTimeout(ctx, timeout)
		err = srv.UploadTestContext(timeoutCtx)
		cancel()
		if err != nil {
			if errors.Is(err, context.Canceled) {
				res.Cancelled = true
			}
			return err
		}
	}

	res.DlSpeed = internal.BrateToStr(float64(srv.DLSpeed))
	res.UlSpeed = internal.BrateToStr(float64(srv.ULSpeed))
	res.DlBytes = srv.Context.GetTotalDownload()
	res.UlBytes = srv.Context.GetTotalUpload()
	res.Latency = int32(srv.Latency.Milliseconds())
	SpTQuerier.storeResult(res)
	return nil
}
