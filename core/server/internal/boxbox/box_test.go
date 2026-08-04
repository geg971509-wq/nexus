package boxbox

import (
	"context"
	"testing"

	"github.com/sagernet/sing-box/include"
)

type contextKey struct{}

func TestNew_preserves_context_when_options_provide_one(t *testing.T) {
	// Given
	want := new(int)
	ctx := context.WithValue(context.Background(), contextKey{}, want)
	ctx = Context(ctx, include.InboundRegistry(), include.OutboundRegistry(), include.EndpointRegistry(), include.DNSTransportRegistry(), include.ServiceRegistry())

	// When
	instance, err := New(Options{Context: ctx})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}
	t.Cleanup(func() {
		if err := instance.Close(); err != nil {
			t.Errorf("Close() error = %v", err)
		}
	})

	// Then
	if got := instance.Context().Value(contextKey{}); got != want {
		t.Errorf("Context() value = %p, want %p", got, want)
	}
}
