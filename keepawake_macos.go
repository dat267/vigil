//go:build darwin

package main

import (
	"fmt"
	"os/exec"
)

func startInhibit() (func(), error) {
	cmd := exec.Command("caffeinate", "-d", "-i")
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("failed to start caffeinate: %w", err)
	}

	cleanup := func() {
		if cmd.Process != nil {
			_ = cmd.Process.Kill()
		}
	}

	return cleanup, nil
}
