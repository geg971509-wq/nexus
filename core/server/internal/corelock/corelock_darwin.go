//go:build darwin

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
	// 0666, not 0600: the same path is opened both by a root Core (setuid, Tun on)
	// and by an ordinary-user Core (Tun off), and the lock file outlives either.
	// At 0600 a leftover root-owned file made the next user-owned Core fail with
	// EACCES once a PID was reused. The containing directory is the user's own
	// 0700 temp dir, so the wider mode grants nothing across users.
	file, err := os.OpenFile(name+".lock", os.O_CREATE|os.O_RDWR, 0o666)
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
