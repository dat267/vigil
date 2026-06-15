package main

import (
	"errors"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"time"
)

func main() {
	if err := run(os.Args[1:]); err != nil {
		fmt.Fprintln(os.Stderr, "Error:", err)
		os.Exit(1)
	}
}

func run(args []string) error {
	fs := flag.NewFlagSet("vigil", flag.ContinueOnError)
	timeoutFlag := fs.String("t", "", "Exit after specified duration (e.g. 2h, 45m, 15s)")
	shutdownFlag := fs.Bool("s", false, "Shutdown the system after the timeout expires")

	if err := fs.Parse(args); err != nil {
		return err
	}

	if *shutdownFlag && *timeoutFlag == "" {
		return errors.New("cannot use -s (shutdown) without specifying a timeout duration via -t")
	}

	stopInhibit, err := startInhibit()
	if err != nil {
		return fmt.Errorf("failed to initialize sleep inhibitor: %w", err)
	}
	defer stopInhibit()

	startTime := time.Now()
	var timeoutChan <-chan time.Time

	if *timeoutFlag != "" {
		dur, err := time.ParseDuration(*timeoutFlag)
		if err != nil {
			return fmt.Errorf("invalid timeout %q", *timeoutFlag)
		}
		timeoutChan = time.After(dur)

		stopTime := startTime.Add(dur).Format(time.DateTime)
		if *shutdownFlag {
			fmt.Printf("Vigil active until %s (with system shutdown).\nPress Ctrl+C to stop.\n", stopTime)
		} else {
			fmt.Printf("Vigil active until %s.\nPress Ctrl+C to stop.\n", stopTime)
		}
	} else {
		fmt.Println("Vigil active indefinitely.\nPress Ctrl+C to stop.")
	}

	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-timeoutChan:
			fmt.Println("\nTimeout reached.")
			if *shutdownFlag {
				return triggerShutdownCountdown()
			}
			return nil
		case <-ticker.C:
			elapsed := time.Since(startTime)
			hours := int(elapsed.Hours())
			minutes := int(elapsed.Minutes()) % 60
			seconds := int(elapsed.Seconds()) % 60

			fmt.Printf("\rElapsed: %02d:%02d:%02d\033[K", hours, minutes, seconds)
		}
	}
}

func triggerShutdownCountdown() error {
	fmt.Println("\nWARNING: Shutdown triggered. Press Ctrl+C to cancel.")

	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	for i := 60; i > 0; i-- {
		<-ticker.C
		fmt.Printf("\rShutting down in %d seconds...\033[K", i)
	}

	fmt.Println("\nShutting down now...")
	var cmd *exec.Cmd
	if runtime.GOOS == "windows" {
		cmd = exec.Command("shutdown", "/s", "/t", "0")
	} else {
		cmd = exec.Command("shutdown", "-h", "now")
	}
	return cmd.Run()
}
