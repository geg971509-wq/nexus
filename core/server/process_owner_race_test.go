//go:build darwin && arm64

package main

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gofrs/uuid/v5"
	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing-box/experimental/clashapi/trafficontrol"
)

// Connections() hands out *TrackerMetadata, so ProcessInfo is shared: the
// enricher's AfterFunc timers write it while QueryConnections reads it from a
// goroutine per IPC request — and connMetaToProto persists ProcessPath on that
// read path, so two concurrent polls race even with no timer running.
//
// Run under -race; before ownerMu this failed on the reader/writer pair and on
// the two readers alone.
func TestProcessInfoAccessIsSynchronized(t *testing.T) {
	id, err := uuid.NewV4()
	if err != nil {
		t.Fatalf("uuid: %v", err)
	}
	tracker := &trafficontrol.TrackerMetadata{
		ID:        id,
		CreatedAt: time.Now(),
		Upload:    new(atomic.Int64),
		Download:  new(atomic.Int64),
		Metadata: adapter.InboundContext{
			ProcessInfo: &adapter.ConnectionOwner{ProcessID: 1234},
		},
	}

	const readers, writers, iters = 4, 2, 200
	var wg sync.WaitGroup

	for i := 0; i < readers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for n := 0; n < iters; n++ {
				if got := connMetaToProto(tracker); got == nil {
					t.Error("connMetaToProto returned nil")
					return
				}
			}
		}()
	}
	// Stands in for attachProcessToTrackers / scheduleOwnerRetries, which swap
	// the pointer wholesale under the same lock.
	for i := 0; i < writers; i++ {
		wg.Add(1)
		go func(base uint32) {
			defer wg.Done()
			for n := 0; n < iters; n++ {
				ownerMu.Lock()
				tracker.Metadata.ProcessInfo = &adapter.ConnectionOwner{
					ProcessID:   base + uint32(n),
					ProcessPath: "/usr/bin/example",
				}
				ownerMu.Unlock()
			}
		}(uint32(i * 10000))
	}

	wg.Wait()
}
