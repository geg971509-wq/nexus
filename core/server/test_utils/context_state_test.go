package test_utils

import (
	"sync"
	"testing"
)

func TestTestContextConcurrentCancelAndRead(t *testing.T) {
	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		for i := 0; i < 10000; i++ {
			_ = CurrentTestContext()
		}
	}()
	go func() {
		defer wg.Done()
		for i := 0; i < 10000; i++ {
			CancelAndResetTests()
		}
	}()
	wg.Wait()
}
