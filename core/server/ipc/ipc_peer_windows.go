//go:build windows

package ipc

import "net"

// VerifyPeerIsParent is a no-op on Windows: the shell side secures the named
// pipe with an explicit DACL (see app/src-tauri/src/core/winpipe.rs), so the
// pipe is not reachable by other users in the first place.
func VerifyPeerIsParent(net.Conn) error { return nil }
