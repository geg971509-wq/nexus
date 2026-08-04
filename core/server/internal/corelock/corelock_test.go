package corelock

import (
	"errors"
	"path/filepath"
	"testing"
)

func TestAcquireRejectsSecondCore(t *testing.T) {
	// Given
	name := filepath.Join(t.TempDir(), "throne-core.lock")
	first, err := Acquire(name)
	if err != nil {
		t.Fatalf("first Acquire() error = %v", err)
	}
	defer first.Close()

	// When
	second, err := Acquire(name)

	// Then
	if second != nil {
		second.Close()
		t.Fatal("second Acquire() returned a lock")
	}
	if !errors.Is(err, ErrAlreadyRunning) {
		t.Fatalf("second Acquire() error = %v, want ErrAlreadyRunning", err)
	}
}
