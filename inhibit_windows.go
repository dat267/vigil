//go:build windows

package main

import (
	"fmt"
	"runtime"
	"syscall"
)

const (
	esContinuous      = 0x80000000
	esSystemRequired  = 0x00000001
	esDisplayRequired = 0x00000002
)

var (
	kernel32                = syscall.NewLazyDLL("kernel32.dll")
	setThreadExecutionState = kernel32.NewProc("SetThreadExecutionState")
)

func startInhibit() (func(), error) {

	// Lock the goroutine to the current OS thread because SetThreadExecutionState is thread-local.
	runtime.LockOSThread()

	ret, _, err := setThreadExecutionState.Call(uintptr(esContinuous | esSystemRequired | esDisplayRequired))
	if ret == 0 {
		runtime.UnlockOSThread()
		return nil, fmt.Errorf("failed to set thread execution state: %w", err)
	}

	cleanup := func() {
		_, _, _ = setThreadExecutionState.Call(uintptr(esContinuous))
		runtime.UnlockOSThread()
	}

	return cleanup, nil
}
