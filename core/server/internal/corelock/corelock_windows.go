//go:build windows

package corelock

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"

	"golang.org/x/sys/windows"
)

type Lock struct {
	handle windows.Handle
}

func Acquire(name string) (*Lock, error) {
	digest := sha256.Sum256([]byte(name))
	mutexName, err := windows.UTF16PtrFromString(`Local\NexusCore-` + hex.EncodeToString(digest[:16]))
	if err != nil {
		return nil, fmt.Errorf("encode core mutex name: %w", err)
	}
	handle, err := windows.CreateMutex(nil, false, mutexName)
	if errors.Is(err, windows.ERROR_ALREADY_EXISTS) {
		windows.CloseHandle(handle)
		return nil, ErrAlreadyRunning
	}
	if err != nil {
		return nil, fmt.Errorf("create core mutex: %w", err)
	}
	return &Lock{handle: handle}, nil
}

func (l *Lock) Close() error {
	if err := windows.CloseHandle(l.handle); err != nil {
		return fmt.Errorf("close core mutex: %w", err)
	}
	return nil
}
