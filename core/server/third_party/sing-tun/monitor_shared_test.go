//go:build linux || windows || darwin

package tun

import (
	"sync"
	"testing"
)

func TestDefaultInterfaceMonitorConcurrentDelayCheckUpdate(t *testing.T) {
	monitor := new(defaultInterfaceMonitor)

	var workers sync.WaitGroup
	for range 100 {
		workers.Add(1)
		go func() {
			defer workers.Done()
			monitor.delayCheckUpdate()
		}()
	}
	workers.Wait()

	monitor.access.Lock()
	monitor.checkUpdateTimer.Stop()
	monitor.access.Unlock()
}
