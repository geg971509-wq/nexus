package main

import (
	"errors"
	"io"
	"net"
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
