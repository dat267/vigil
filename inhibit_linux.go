//go:build linux

package main

import (
	"fmt"
	"os/exec"
	"syscall"
)

func startInhibit() (func(), error) {
	// Pre-flight dry-run check: verify systemd-inhibit is available and has necessary permissions.
	// This prevents silent failure (e.g. if PolicyKit denies access or prompts for password).
	dryRun := exec.Command("systemd-inhibit", "--what=idle:sleep", "--who=vigil", "--why=check", "true")
	if err := dryRun.Run(); err != nil {
		return nil, fmt.Errorf("failed to obtain sleep inhibition lock (permission denied or systemd-inhibit is missing). Try running with sudo. Error: %w", err)
	}

	// Prevents both system suspension (sleep) and idle screen blanking
	cmd := exec.Command("systemd-inhibit", "--what=idle:sleep", "--who=vigil", "--why=Inhibiting sleep", "sleep", "365d")

	// Ensure the systemd-inhibit process is terminated if vigil exits or crashes
	cmd.SysProcAttr = &syscall.SysProcAttr{
		Pdeathsig: syscall.SIGTERM,
	}

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("failed to start systemd-inhibit: %w", err)
	}

	cleanup := func() {
		if cmd.Process != nil {
			_ = cmd.Process.Kill()
			_ = cmd.Wait() // Reaps the child process to prevent zombie states
		}
	}

	return cleanup, nil
}
