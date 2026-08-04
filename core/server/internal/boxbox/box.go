package boxbox

import (
	"context"

	box "github.com/sagernet/sing-box"
	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing/service"
)

type Box struct {
	*box.Box
	ctx context.Context
}

type Options = box.Options

func Context(
	ctx context.Context,
	inboundRegistry adapter.InboundRegistry,
	outboundRegistry adapter.OutboundRegistry,
	endpointRegistry adapter.EndpointRegistry,
	dnsTransportRegistry adapter.DNSTransportRegistry,
	serviceRegistry adapter.ServiceRegistry,
) context.Context {
	return box.Context(ctx, inboundRegistry, outboundRegistry, endpointRegistry, dnsTransportRegistry, serviceRegistry)
}

func New(options Options) (*Box, error) {
	ctx := options.Context
	if ctx == nil {
		ctx = context.Background()
	}
	ctx = service.ContextWithDefaultRegistry(ctx)
	options.Context = ctx

	instance, err := box.New(options)
	if err != nil {
		return nil, err
	}
	return &Box{Box: instance, ctx: ctx}, nil
}

func (s *Box) Context() context.Context {
	return s.ctx
}
