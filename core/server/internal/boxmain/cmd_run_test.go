package boxmain

import (
	"context"
	"strings"
	"testing"
)

func TestParseConfigErrorDoesNotEchoConfig(t *testing.T) {
	const secret = "nexus-release-secret"
	_, err := parseConfig(context.Background(), []byte(`{"password":"`+secret+`"`))
	if err == nil {
		t.Fatal("parseConfig returned nil error for malformed JSON")
	}
	if strings.Contains(err.Error(), secret) {
		t.Fatalf("parse error leaked config content: %v", err)
	}
}
