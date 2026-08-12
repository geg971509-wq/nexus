//go:build !darwin

package main

// processPathFromPID: non-darwin path fill is handled inside sing-box searchers.
func processPathFromPID(pid uint32) string {
	_ = pid
	return ""
}
