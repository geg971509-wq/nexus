#!/usr/bin/env bash
# Generate Go protobuf stubs from core/server/gen/libcore.proto.
# Called by build.sh and CI — the generated *.pb.go are gitignored, so a clean
# clone must be able to produce them or GPLv3 Corresponding Source cannot
# rebuild Core.
#
# The wire is our own framed IPC (dispatch.go), but the grpc stubs are still
# required: server.go embeds gen.UnimplementedLibcoreServiceServer to supply
# default methods for RPCs implemented on one OS only (e.g. SetSystemDNS is
# Windows-only, yet dispatch.go calls it unconditionally).
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CORE_SRC="$ROOT/core/server"

command -v protoc >/dev/null 2>&1 || { echo "missing command: protoc" >&2; exit 1; }

cd "$CORE_SRC"
# Pin protoc-gen-go to the version in go.mod: Corresponding Source must rebuild
# the same Core, and @latest would drift. protoc-gen-go-grpc versions
# independently of the grpc module, so it carries its own pin.
# Separate invocations: `go install` rejects multiple args at different versions.
pb_ver="$(go list -m -f '{{.Version}}' google.golang.org/protobuf)"
GOBIN="$ROOT/bin/tools" go install "google.golang.org/protobuf/cmd/protoc-gen-go@${pb_ver}"
GOBIN="$ROOT/bin/tools" go install \
  "google.golang.org/grpc/cmd/protoc-gen-go-grpc@${PROTOC_GEN_GO_GRPC_VERSION:-v1.5.1}"
PATH="$ROOT/bin/tools:$PATH" protoc \
  --proto_path=gen \
  --go_out=gen --go_opt=paths=source_relative \
  --go-grpc_out=gen --go-grpc_opt=paths=source_relative \
  gen/libcore.proto

[[ -f "$CORE_SRC/gen/libcore.pb.go" ]] || { echo "protoc did not produce gen/libcore.pb.go" >&2; exit 1; }
