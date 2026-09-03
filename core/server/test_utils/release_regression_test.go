package test_utils

import (
	"NexusCore/internal/boxbox"
	"context"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/sagernet/sing-box/include"
)

func newEmptyTestBox(t *testing.T) *boxbox.Box {
	t.Helper()
	ctx := boxbox.Context(
		context.Background(),
		include.InboundRegistry(),
		include.OutboundRegistry(),
		include.EndpointRegistry(),
		include.DNSTransportRegistry(),
		include.ServiceRegistry(),
	)
	instance, err := boxbox.New(boxbox.Options{Context: ctx})
	if err != nil {
		t.Fatalf("boxbox.New: %v", err)
	}
	t.Cleanup(func() {
		if err := instance.Close(); err != nil {
			t.Errorf("box close: %v", err)
		}
	})
	return instance
}

func requireMissingOutboundResult(t *testing.T, tag string, err error) {
	t.Helper()
	if err == nil {
		t.Fatal("missing outbound returned nil error")
	}
	if !strings.Contains(err.Error(), tag) {
		t.Fatalf("error %q does not mention tag %q", err, tag)
	}
}

func TestBatchIPTestMissingOutboundReturnsError(t *testing.T) {
	const tag = "missing-ip"
	results := BatchIPTest(context.Background(), newEmptyTestBox(t), []string{tag}, 1, time.Millisecond)
	if len(results) != 1 || results[0] == nil {
		t.Fatalf("results = %#v, want one result", results)
	}
	if results[0].Tag != tag {
		t.Fatalf("tag = %q, want %q", results[0].Tag, tag)
	}
	requireMissingOutboundResult(t, tag, results[0].Error)
}

func TestBatchURLTestMissingOutboundReturnsError(t *testing.T) {
	const tag = "missing-url"
	results := BatchURLTest(context.Background(), newEmptyTestBox(t), []string{tag}, "http://127.0.0.1/", 1, false, time.Millisecond)
	if len(results) != 1 || results[0] == nil {
		t.Fatalf("results = %#v, want one result", results)
	}
	if results[0].Tag != tag {
		t.Fatalf("tag = %q, want %q", results[0].Tag, tag)
	}
	requireMissingOutboundResult(t, tag, results[0].Error)
}

func TestBatchURLTestNegativeConcurrencyUsesDefault(t *testing.T) {
	const tag = "missing-negative-concurrency"
	results := BatchURLTest(context.Background(), newEmptyTestBox(t), []string{tag}, "http://127.0.0.1/", -1, false, time.Millisecond)
	if len(results) != 1 || results[0] == nil {
		t.Fatalf("results = %#v, want one result", results)
	}
	requireMissingOutboundResult(t, tag, results[0].Error)
}

func TestBatchSpeedTestMissingOutboundReturnsError(t *testing.T) {
	const tag = "missing-speed"
	results := BatchSpeedTest(context.Background(), newEmptyTestBox(t), []string{tag}, false, false, false, "", time.Millisecond, false, 0)
	if len(results) != 1 || results[0] == nil {
		t.Fatalf("results = %#v, want one result", results)
	}
	if results[0].Tag != tag {
		t.Fatalf("tag = %q, want %q", results[0].Tag, tag)
	}
	requireMissingOutboundResult(t, tag, results[0].Error)
}

func TestBatchSpeedTestCancelledBeforeLoopDoesNotScheduleTags(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	results := BatchSpeedTest(ctx, newEmptyTestBox(t), []string{"must-not-be-looked-up"}, false, false, false, "", time.Millisecond, false, 0)
	if len(results) != 0 {
		t.Fatalf("results = %#v, want none after pre-cancel", results)
	}
}

func TestSpeedTestResultQuerierConcurrentRunningState(t *testing.T) {
	var q SpeedTestResultQuerier
	start := make(chan struct{})
	var wg sync.WaitGroup
	wg.Add(2)

	go func() {
		defer wg.Done()
		<-start
		for i := 0; i < 100_000; i++ {
			q.setIsRunning(i%2 == 0)
		}
	}()
	go func() {
		defer wg.Done()
		<-start
		for i := 0; i < 100_000; i++ {
			_, _ = q.Result()
		}
	}()

	close(start)
	wg.Wait()
}
