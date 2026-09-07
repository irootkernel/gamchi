package main

import (
	"bytes"
	"strings"
	"testing"
)

func TestVersion(t *testing.T) {
	stdout, stderr := &bytes.Buffer{}, &bytes.Buffer{}
	if code := run([]string{"version"}, stdout, stderr); code != 0 {
		t.Fatalf("exit %d stderr %q", code, stderr.String())
	}
	if got := strings.TrimSpace(stdout.String()); got != "grokgrok/dev" {
		t.Fatalf("stdout %q", got)
	}
	if stderr.Len() != 0 {
		t.Fatalf("stderr %q", stderr.String())
	}
}

func TestNoArgs(t *testing.T) {
	stdout, stderr := &bytes.Buffer{}, &bytes.Buffer{}
	if code := run(nil, stdout, stderr); code != 2 {
		t.Fatalf("exit %d", code)
	}
	if stdout.Len() != 0 {
		t.Fatalf("stdout %q", stdout.String())
	}
	if !strings.Contains(stderr.String(), "grokgrok version") {
		t.Fatalf("stderr %q", stderr.String())
	}
}

func TestUnknownCommand(t *testing.T) {
	stdout, stderr := &bytes.Buffer{}, &bytes.Buffer{}
	if code := run([]string{"nope"}, stdout, stderr); code != 1 {
		t.Fatalf("exit %d", code)
	}
	if !strings.Contains(stderr.String(), "GROKGROK_STARTUP_ERROR INVALID_CONFIG") {
		t.Fatalf("stderr %q", stderr.String())
	}
}

func TestUnimplementedSurfaces(t *testing.T) {
	for _, cmd := range []string{"worker", "mcp", "app-server"} {
		stdout, stderr := &bytes.Buffer{}, &bytes.Buffer{}
		if code := run([]string{cmd}, stdout, stderr); code != 1 {
			t.Fatalf("%s: exit %d", cmd, code)
		}
		if !strings.Contains(stderr.String(), "GROKGROK_STARTUP_ERROR NOT_IMPLEMENTED "+cmd) {
			t.Fatalf("%s: stderr %q", cmd, stderr.String())
		}
	}
}
