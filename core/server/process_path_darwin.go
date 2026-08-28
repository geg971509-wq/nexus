//go:build darwin && arm64

package main

import (
	"syscall"
	"unsafe"

	"golang.org/x/sys/unix"
)

// processPathFromPID: root (setuid Tun Core) can resolve most paths via proc_pidpath.
// Non-root may still resolve same-user processes; SIP-protected targets can fail.
func processPathFromPID(pid uint32) string {
	if pid == 0 {
		return ""
	}
	const (
		procpidpathinfo     = 0xb
		procpidpathinfosize = 1024
		proccallnumpidinfo  = 0x2
	)
	buf := make([]byte, procpidpathinfosize)
	_, _, errno := syscall.Syscall6(
		syscall.SYS_PROC_INFO,
		proccallnumpidinfo,
		uintptr(pid),
		procpidpathinfo,
		0,
		uintptr(unsafe.Pointer(&buf[0])),
		procpidpathinfosize,
	)
	if errno != 0 {
		return ""
	}
	return unix.ByteSliceToString(buf)
}
