//go:build darwin && arm64

package main

import (
	"context"
	"testing"

	"NexusCore/gen"
)

func TestQueryStateReportsActiveProfile(t *testing.T) {
	// Given
	activeProfileID.Store(42)
	t.Cleanup(func() { activeProfileID.Store(-1) })

	// When
	state, err := globalServer.QueryState(context.Background(), &gen.EmptyReq{})
	// Then
	if err != nil {
		t.Fatalf("QueryState() error = %v", err)
	}
	if !state.GetRunning() || state.GetProfileId() != 42 {
		t.Fatalf("QueryState() = running %v, profile %d", state.GetRunning(), state.GetProfileId())
	}
}
