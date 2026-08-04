package main

import (
	"context"
	"errors"
	"fmt"
	"log"
	"os"
	"runtime"
	runtimeDebug "runtime/debug"
	"time"

	"ThroneCore/internal/boxmain"
	"ThroneCore/internal/corelock"
	"ThroneCore/ipc"
	"ThroneCore/test_utils"

	"github.com/xtls/xray-core/core"

	_ "ThroneCore/internal/distro/all"
	C "github.com/sagernet/sing-box/constant"
)

func RunCore() {
	socketName := os.Getenv("THRONE_CORE_SOCKET")
	if socketName == "" {
		log.Fatal("THRONE_CORE_SOCKET not set")
	}
	debug = os.Getenv("THRONE_CORE_DEBUG") == "1"

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

		fmt.Println("Core Has Successfully Connected to Throne!")
		if dispatchErr := runDispatch(conn); dispatchErr != nil && debug {
			log.Printf("GUI IPC disconnected: %v", dispatchErr)
		}
	}
}

func main() {
	defer func() {
		if err := recover(); err != nil {
			fmt.Println("Core panicked:")
			fmt.Println(err)
			os.Exit(0)
		}
	}()
	fmt.Println("sing-box:", C.Version)
	fmt.Println("Xray-core:", core.Version())
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

	test_utils.TestCtx, test_utils.CancelTests = context.WithCancel(context.Background())
	RunCore()
	return
}
