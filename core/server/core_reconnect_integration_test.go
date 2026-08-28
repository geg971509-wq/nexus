//go:build darwin || linux

package main

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"testing"
	"time"

	"NexusCore/gen"

	"google.golang.org/protobuf/proto"
)

func TestCoreKeepsListenerAcrossGUIReconnect(t *testing.T) {
	if os.Getenv("NEXUS_CORE_RECONNECT_HELPER") == "1" {
		RunCore()
		return
	}

	// Given
	socketPath := fmt.Sprintf("/tmp/upstream-core-%d-%d.sock", os.Getpid(), time.Now().UnixNano())
	t.Cleanup(func() {
		_ = os.Remove(socketPath)
		_ = os.Remove(socketPath + ".lock")
	})
	listener := listenCoreSocket(t, socketPath)
	port := unusedTCPPort(t)
	var output bytes.Buffer
	cmd := exec.Command(os.Args[0], "-test.run=^TestCoreKeepsListenerAcrossGUIReconnect$")
	cmd.Env = append(os.Environ(),
		"NEXUS_CORE_RECONNECT_HELPER=1",
		"NEXUS_CORE_SOCKET="+socketPath,
	)
	cmd.Stdout = &output
	cmd.Stderr = &output
	if err := cmd.Start(); err != nil {
		t.Fatalf("start core helper: %v", err)
	}
	corePID := cmd.Process.Pid
	t.Cleanup(func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		if t.Failed() {
			t.Logf("core output:\n%s", output.String())
		}
	})

	firstConn := acceptCore(t, listener)
	config := fmt.Sprintf(`{
		"log":{"disabled":true},
		"inbounds":[{"type":"socks","tag":"socks-in","listen":"127.0.0.1","listen_port":%d}],
		"outbounds":[{"type":"direct","tag":"direct"}],
		"route":{"final":"direct"}
	}`, port)
	start := &gen.LoadConfigReq{
		CoreConfig:       &config,
		NeedExtraProcess: boolPtr(false),
		ProfileId:        int32Ptr(42),
	}
	startPayload := callCore(t, firstConn, 1, "Start", start)
	startResponse := &gen.ErrorResp{}
	if err := proto.Unmarshal(startPayload, startResponse); err != nil {
		t.Fatalf("decode Start response: %v", err)
	}
	if startResponse.GetError() != "" {
		t.Fatalf("start core: %s\n%s", startResponse.GetError(), output.String())
	}
	assertTCPListening(t, port)

	// When
	_ = firstConn.Close()
	_ = listener.Close()
	time.Sleep(900 * time.Millisecond)
	listener = listenCoreSocket(t, socketPath)
	reconnectedAt := time.Now()
	secondConn := acceptCore(t, listener)
	if delay := time.Since(reconnectedAt); delay > 750*time.Millisecond {
		t.Fatalf("core reconnect took %s", delay)
	}

	// Then
	if cmd.Process.Pid != corePID {
		t.Fatalf("core PID changed from %d to %d", corePID, cmd.Process.Pid)
	}
	statePayload := callCore(t, secondConn, 2, "QueryState", &gen.EmptyReq{})
	state := &gen.CoreStateResponse{}
	if err := proto.Unmarshal(statePayload, state); err != nil {
		t.Fatalf("decode QueryState response: %v", err)
	}
	if !state.GetRunning() || state.GetProfileId() != 42 {
		t.Fatalf("QueryState() = running %v, profile %d", state.GetRunning(), state.GetProfileId())
	}
	assertTCPListening(t, port)
	_ = callCore(t, secondConn, 3, "Stop", &gen.EmptyReq{})
}

func listenCoreSocket(t *testing.T, path string) *net.UnixListener {
	t.Helper()
	_ = os.Remove(path)
	listener, err := net.ListenUnix("unix", &net.UnixAddr{Name: path, Net: "unix"})
	if err != nil {
		t.Fatalf("listen on %s: %v", path, err)
	}
	return listener
}

func acceptCore(t *testing.T, listener *net.UnixListener) net.Conn {
	t.Helper()
	if err := listener.SetDeadline(time.Now().Add(10 * time.Second)); err != nil {
		t.Fatalf("set listener deadline: %v", err)
	}
	conn, err := listener.Accept()
	if err != nil {
		t.Fatalf("accept core: %v", err)
	}
	if err := conn.SetDeadline(time.Now().Add(10 * time.Second)); err != nil {
		t.Fatalf("set core connection deadline: %v", err)
	}
	return conn
}

func unusedTCPPort(t *testing.T) int {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("reserve TCP port: %v", err)
	}
	defer listener.Close()
	return listener.Addr().(*net.TCPAddr).Port
}

func assertTCPListening(t *testing.T, port int) {
	t.Helper()
	conn, err := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", port), time.Second)
	if err != nil {
		t.Fatalf("connect to core listener: %v", err)
	}
	_ = conn.Close()
}

func callCore(t *testing.T, conn net.Conn, requestID uint32, method string, request proto.Message) []byte {
	t.Helper()
	payload, err := proto.Marshal(request)
	if err != nil {
		t.Fatalf("encode %s request: %v", method, err)
	}
	var frame bytes.Buffer
	_ = binary.Write(&frame, binary.LittleEndian, requestID)
	_ = binary.Write(&frame, binary.LittleEndian, uint16(len(method)))
	_, _ = frame.WriteString(method)
	_ = binary.Write(&frame, binary.LittleEndian, uint32(len(payload)))
	_, _ = frame.Write(payload)
	if _, err := conn.Write(frame.Bytes()); err != nil {
		t.Fatalf("write %s request: %v", method, err)
	}

	var responseID uint32
	var status uint8
	var length uint32
	if err := binary.Read(conn, binary.LittleEndian, &responseID); err != nil {
		t.Fatalf("read %s response ID: %v", method, err)
	}
	if err := binary.Read(conn, binary.LittleEndian, &status); err != nil {
		t.Fatalf("read %s status: %v", method, err)
	}
	if err := binary.Read(conn, binary.LittleEndian, &length); err != nil {
		t.Fatalf("read %s response length: %v", method, err)
	}
	response := make([]byte, length)
	if _, err := io.ReadFull(conn, response); err != nil {
		t.Fatalf("read %s response: %v", method, err)
	}
	if responseID != requestID || status != 0 {
		t.Fatalf("%s response = id %d status %d payload %q", method, responseID, status, response)
	}
	return response
}

func int32Ptr(value int32) *int32 {
	return &value
}

func boolPtr(value bool) *bool {
	return &value
}
