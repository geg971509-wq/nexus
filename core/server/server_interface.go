package main

import (
	"NexusCore/gen"
	"NexusCore/internal/boxdns"
	"context"
	"errors"
)

// GetDefaultInterface reports the physical default-route interface tracked by
// the always-on boxdns monitor. The monitor runs on every platform and excludes
// virtual (TUN) and loopback interfaces, so the result is safe to bind a core
// egress to even while upstream-tun is up.
func (s *server) GetDefaultInterface(ctx context.Context, in *gen.EmptyReq) (*gen.GetDefaultInterfaceResponse, error) {
	ifc := boxdns.DefaultInterface()
	if ifc == nil {
		return nil, errors.New("no default interface")
	}
	return &gen.GetDefaultInterfaceResponse{
		Name:  To(ifc.Name),
		Index: To(int32(ifc.Index)),
	}, nil
}
