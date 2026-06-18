//go:build linux

package main

import (
	"fmt"
	"os/exec"
	"syscall"
)

func startInhibit() (func(), error) {
	// Prevents both system suspension (sleep) and idle screen blanking
	cmd := exec.Command("systemd-inhibit", "--what=idle:sleep", "--who=vigil", "--why=Inhibiting sleep", "sleep", "365d")

	// Ensure the systemd-inhibit process is terminated if vigil exits or crashes
	cmd.SysProcAttr = &syscall.SysProcAttr{
		Pdeathsig: syscall.SIGTERM,
	}

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("failed to start systemd-inhibit (check if systemd is installed): %w", err)
	}

	cleanup := func() {
		if cmd.Process != nil {
			_ = cmd.Process.Kill()
			_ = cmd.Wait() // Reaps the child process to prevent zombie states
		}
	}

	return cleanup, nil
}
