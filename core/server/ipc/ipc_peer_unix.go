//go:build linux || darwin

package ipc

import (
	"fmt"
	"net"
	"os"
)

// VerifyPeerIsParent fails when the IPC server we connected to is not the
// process that spawned this core. The shell creates the socket 0600 inside a
// directory it chmods to 0700, so this is defense in depth: it closes the window
// where a stale or hijacked socket path points at another local process that
// would then drive privileged Start/Stop.
func VerifyPeerIsParent(conn net.Conn) error {
	uc, ok := conn.(*net.UnixConn)
	if !ok {
		return fmt.Errorf("peer check: not a unix conn")
	}
	raw, err := uc.SyscallConn()
	if err != nil {
		return fmt.Errorf("peer check: raw conn: %w", err)
	}
	var pid int
	var pidErr error
	if err := raw.Control(func(fd uintptr) {
		pid, pidErr = getServerPid(int(fd))
	}); err != nil {
		return fmt.Errorf("peer check: control: %w", err)
	}
	if pidErr != nil {
		return fmt.Errorf("peer check: peer pid: %w", pidErr)
	}
	if want := os.Getppid(); pid != want {
		return fmt.Errorf("peer check: server pid %d is not parent %d", pid, want)
	}
	return nil
}
