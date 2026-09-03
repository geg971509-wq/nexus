//go:build darwin && arm64

package main

import (
	"errors"
	"fmt"
	"log"
	"os"
	"runtime"
	runtimeDebug "runtime/debug"
	"time"

	"NexusCore/internal/boxmain"
	"NexusCore/internal/corelock"
	"NexusCore/ipc"

	C "github.com/sagernet/sing-box/constant"
)

func RunCore() {
	socketName := os.Getenv("NEXUS_CORE_SOCKET")
	if socketName == "" {
		log.Fatal("NEXUS_CORE_SOCKET not set")
	}
	debug = os.Getenv("NEXUS_CORE_DEBUG") == "1"

	lock, err := corelock.Acquire(socketName)
	if errors.Is(err, corelock.ErrAlreadyRunning) {
		return
	}
	if err != nil {
		log.Fatalf("acquire core lock: %v", err)
	}
	defer lock.Close() //nolint:errcheck -- process exit releases the OS lock

	boxmain.DisableColor()

	retryDelay := 250 * time.Millisecond
	for {
		conn, connectErr := ipc.ConnectIPC(socketName)
		if connectErr != nil {
			time.Sleep(retryDelay)
			retryDelay = min(retryDelay*2, 500*time.Millisecond)
			continue
		}
		retryDelay = 250 * time.Millisecond

		if peerErr := ipc.VerifyPeerIsParent(conn); peerErr != nil {
			conn.Close() //nolint:errcheck -- already failing this connection
			log.Fatalf("refusing IPC peer: %v", peerErr)
		}

		fmt.Println("Core Has Successfully Connected to Nexus!")
		if dispatchErr := runDispatch(conn); dispatchErr != nil && debug {
			log.Printf("GUI IPC disconnected: %v", dispatchErr)
		}
	}
}

func main() {
	fmt.Println("sing-box:", C.Version)
	fmt.Println()
	runtimeDebug.SetMemoryLimit(2 * 1024 * 1024 * 1024) // 2GB
	go func() {
		var memStats runtime.MemStats
		for {
			time.Sleep(2 * time.Second)
			runtime.ReadMemStats(&memStats)
			if memStats.HeapAlloc > 1.5*1024*1024*1024 {
				panic("Memory has reached 1.5 GB, this is not normal")
			}
		}
	}()

	// Do not recover process-level panics as a successful exit. Request-handler
	// panics are contained by runDispatch; anything escaping to the process is a
	// real Core failure and must retain a non-zero status for diagnostics.
	RunCore()
}
