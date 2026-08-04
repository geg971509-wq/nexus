package corelock

import "errors"

var ErrAlreadyRunning = errors.New("core already running")
