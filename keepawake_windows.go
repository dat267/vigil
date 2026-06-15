//go:build windows

package main

import (
	"fmt"
	"syscall"
)

const (
	esContinuous      = 0x80000000
	esSystemRequired  = 0x00000001
	esDisplayRequired = 0x00000002
)

func startInhibit() (func(), error) {
	kernel32 := syscall.NewLazyDLL("kernel32.dll")
	setThreadExecutionState := kernel32.NewProc("SetThreadExecutionState")

	ret, _, err := setThreadExecutionState.Call(uintptr(esContinuous | esSystemRequired | esDisplayRequired))
	if ret == 0 {
		return nil, fmt.Errorf("failed to set thread execution state: %w", err)
	}

	cleanup := func() {
		_, _, _ = setThreadExecutionState.Call(uintptr(esContinuous))
	}

	return cleanup, nil
}
