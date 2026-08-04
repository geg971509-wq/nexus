//go:build linux || darwin

package corelock

import (
	"errors"
	"fmt"
	"os"
	"syscall"
)

type Lock struct {
	file *os.File
}

func Acquire(name string) (*Lock, error) {
	file, err := os.OpenFile(name+".lock", os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, fmt.Errorf("open core lock: %w", err)
	}
	if err := syscall.Flock(int(file.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		file.Close()
		if errors.Is(err, syscall.EWOULDBLOCK) {
			return nil, ErrAlreadyRunning
		}
		return nil, fmt.Errorf("lock core: %w", err)
	}
	return &Lock{file: file}, nil
}

func (l *Lock) Close() error {
	unlockErr := syscall.Flock(int(l.file.Fd()), syscall.LOCK_UN)
	if unlockErr != nil {
		unlockErr = fmt.Errorf("unlock core: %w", unlockErr)
	}
	return errors.Join(unlockErr, l.file.Close())
}
