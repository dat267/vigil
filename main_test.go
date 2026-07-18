package main

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// buildBinary compiles the vigil binary into a temp dir and returns its path.
func buildBinary(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	bin := filepath.Join(dir, "vigil")
	cmd := exec.Command("go", "build", "-o", bin, ".")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("failed to build binary: %v", err)
	}
	return bin
}

// run executes the binary with the given args and returns stdout, stderr, and
// whether the process exited successfully.
func run(bin string, args ...string) (stdout, stderr string, ok bool) {
	var outBuf, errBuf bytes.Buffer
	cmd := exec.Command(bin, args...)
	cmd.Stdout = &outBuf
	cmd.Stderr = &errBuf
	err := cmd.Run()
	return outBuf.String(), errBuf.String(), err == nil
}

// ---------------------------------------------------------------------------

func TestHelp(t *testing.T) {
	bin := buildBinary(t)

	for _, args := range [][]string{
		{"--help"},
		{"-h"},
		{"help"},
	} {
		stdout, stderr, ok := run(bin, args...)
		combined := stdout + stderr
		if !ok {
			// kong exits 0 for --help, but let's just check output content
			// regardless of exit code when the output looks like help
			if !strings.Contains(combined, "start") {
				t.Errorf("args %v: expected help output, got: %q", args, combined)
			}
			continue
		}
		if !strings.Contains(combined, "start") {
			t.Errorf("args %v: expected help output containing 'start', got: %q", args, combined)
		}
	}
}

func TestVersion(t *testing.T) {
	bin := buildBinary(t)
	stdout, _, ok := run(bin, "version")
	if !ok {
		t.Fatal("version command failed")
	}
	if !strings.HasPrefix(stdout, "vigil ") {
		t.Errorf("expected 'vigil <version>', got: %q", stdout)
	}
}

func TestStartHelp(t *testing.T) {
	bin := buildBinary(t)
	stdout, stderr, _ := run(bin, "start", "--help")
	combined := stdout + stderr
	if !strings.Contains(combined, "-t") {
		t.Errorf("expected start help to mention -t, got: %q", combined)
	}
	if !strings.Contains(combined, "-s") {
		t.Errorf("expected start help to mention -s, got: %q", combined)
	}
}

func TestStartShutdownWithoutTimeout(t *testing.T) {
	bin := buildBinary(t)
	// Mocking startInhibit is not possible at binary level, but we can verify
	// that -s without -t is rejected before the inhibitor is even started.
	// On Linux the dry-run will fail too, so we just check for a non-zero exit.
	_, stderr, ok := run(bin, "start", "-s")
	if ok {
		t.Fatal("expected non-zero exit for -s without -t")
	}
	// The validation error should mention the flag dependency.
	if !strings.Contains(stderr, "-s") && !strings.Contains(stderr, "shutdown") {
		t.Errorf("expected error mentioning -s or shutdown, got: %q", stderr)
	}
}

func TestStartInvalidTimeout(t *testing.T) {
	bin := buildBinary(t)
	_, _, ok := run(bin, "start", "-t", "notaduration")
	if ok {
		t.Fatal("expected non-zero exit for invalid -t value")
	}
}

func TestUnknownCommand(t *testing.T) {
	bin := buildBinary(t)
	_, _, ok := run(bin, "nonexistent")
	if ok {
		t.Fatal("expected non-zero exit for unknown command")
	}
}
