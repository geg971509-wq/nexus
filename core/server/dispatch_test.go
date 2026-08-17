package main

import (
	"encoding/binary"
	"errors"
	"io"
	"net"
	"strings"
	"testing"
	"time"
)

func TestRunDispatchReturnsWhenConnectionDrops(t *testing.T) {
	// Given
	serverConn, clientConn := net.Pipe()
	done := make(chan error, 1)
	go func() {
		done <- runDispatch(serverConn)
	}()

	// When
	if err := clientConn.Close(); err != nil {
		t.Fatalf("close client connection: %v", err)
	}

	// Then
	select {
	case err := <-done:
		if !errors.Is(err, io.EOF) {
			t.Fatalf("runDispatch() error = %v, want EOF", err)
		}
	case <-time.After(time.Second):
		t.Fatal("runDispatch did not return after the connection dropped")
	}
}

// An oversized payloadLen must be rejected on the header, before make([]byte, n).
// Without the bound, this frame allocates 4 GiB and trips the heap watchdog,
// which kills the privileged core and drops the tunnel.
func TestRunDispatchRejectsOversizedPayload(t *testing.T) {
	// Given
	serverConn, clientConn := net.Pipe()
	done := make(chan error, 1)
	go func() {
		done <- runDispatch(serverConn)
	}()

	// When: reqId=1, method="X", payloadLen=0xFFFFFFFF (no payload follows)
	frame := make([]byte, 0, 11)
	frame = binary.LittleEndian.AppendUint32(frame, 1)
	frame = binary.LittleEndian.AppendUint16(frame, 1)
	frame = append(frame, 'X')
	frame = binary.LittleEndian.AppendUint32(frame, 0xFFFFFFFF)
	go clientConn.Write(frame) //nolint:errcheck -- reader closes on reject

	// Then
	select {
	case err := <-done:
		if err == nil || !strings.Contains(err.Error(), "payload too large") {
			t.Fatalf("runDispatch() error = %v, want payload too large", err)
		}
	case <-time.After(time.Second):
		t.Fatal("runDispatch accepted an oversized payload length")
	}
}
