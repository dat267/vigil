//go:build darwin

package main

import (
	"fmt"
	"os"
	"os/exec"
	"strconv"
)

func startInhibit() (func(), error) {
	// Prevents sleep and idle display blanking.
	// -w specifies the parent PID to watch: caffeinate exits automatically when the parent exits.
	cmd := exec.Command("caffeinate", "-d", "-i", "-w", strconv.Itoa(os.Getpid()))
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("failed to start caffeinate: %w", err)
	}

	cleanup := func() {
		if cmd.Process != nil {
			_ = cmd.Process.Kill()
			_ = cmd.Wait()
		}
	}

	return cleanup, nil
}
