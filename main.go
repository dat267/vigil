package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"runtime"
	"syscall"
	"time"
)

var version = "dev"

func main() {
	if err := run(os.Args[1:]); err != nil {
		fmt.Fprintln(os.Stderr, "Error:", err)
		os.Exit(1)
	}
}

func printHelp() {
	fmt.Print(`vigil — keep your system awake

USAGE
  vigil <command> [flags]

COMMANDS
  start     Start the sleep inhibitor
  version   Show version information
  help      Show this help message

Run 'vigil <command> -h' for command-specific help.

EXAMPLES
  vigil start               Keep awake indefinitely (Ctrl+C to stop)
  vigil start -t 2h         Keep awake for 2 hours, then exit
  vigil start -t 45m -s     Keep awake for 45 minutes, then shut down

`)
}

func printVersion() {
	fmt.Printf("vigil %s\n", version)
}

func isTerminal(f *os.File) bool {
	stat, err := f.Stat()
	if err != nil {
		return false
	}
	return (stat.Mode() & os.ModeCharDevice) != 0
}

var startInhibitFn = startInhibit

func run(args []string) error {
	if len(args) == 0 {
		printHelp()
		return nil
	}

	switch args[0] {
	case "start":
		return cmdStart(args[1:])
	case "version", "-v", "--version":
		printVersion()
		return nil
	case "help", "-h", "--help":
		printHelp()
		return nil
	default:
		printHelp()
		return fmt.Errorf("unknown command %q", args[0])
	}
}

func cmdStart(args []string) error {
	fs := flag.NewFlagSet("vigil start", flag.ContinueOnError)
	timeoutFlag := fs.String("t", "", "Duration to stay awake (e.g. 2h, 45m, 30s, 1h30m)")
	shutdownFlag := fs.Bool("s", false, "Shut down the system when the timeout expires")

	fs.Usage = func() {
		fmt.Fprint(os.Stderr, `vigil start — start the sleep inhibitor

USAGE
  vigil start [flags]

FLAGS
  -t <duration>   Stay awake for the given duration, then exit.
                  Accepts Go duration strings: e.g. 30s, 45m, 2h, 1h30m.
                  Omit to run indefinitely.

  -s              Shut down the system after the timeout (-t) expires.
                  Requires -t; cannot be used alone.

  -h              Show this help message.

EXAMPLES
  vigil start               Keep awake indefinitely (Ctrl+C to stop)
  vigil start -t 2h         Keep awake for 2 hours, then exit
  vigil start -t 1h30m      Keep awake for 1 hour 30 minutes
  vigil start -t 45m -s     Keep awake for 45 minutes, then shut down

`)
	}

	if err := fs.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return nil
		}
		return err
	}

	if *shutdownFlag && *timeoutFlag == "" {
		return errors.New("cannot use -s (shutdown) without specifying a timeout duration via -t")
	}

	var dur time.Duration
	if *timeoutFlag != "" {
		var err error
		dur, err = time.ParseDuration(*timeoutFlag)
		if err != nil {
			return fmt.Errorf("invalid timeout %q: %w", *timeoutFlag, err)
		}
		if dur <= 0 {
			return errors.New("timeout duration must be positive")
		}
	}

	stopInhibit, err := startInhibitFn()
	if err != nil {
		return fmt.Errorf("failed to initialize sleep inhibitor: %w", err)
	}
	defer stopInhibit()

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	startTime := time.Now()
	var timeoutChan <-chan time.Time

	if dur > 0 {
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

	useTicker := isTerminal(os.Stdout)
	var ticker *time.Ticker
	var tickerChan <-chan time.Time

	if useTicker {
		ticker = time.NewTicker(1 * time.Second)
		defer ticker.Stop()
		tickerChan = ticker.C
	}

	for {
		select {
		case <-ctx.Done():
			if useTicker {
				fmt.Println()
			}
			fmt.Println("Stopping vigil...")
			return nil
		case <-timeoutChan:
			if useTicker {
				fmt.Println()
			}
			fmt.Println("Timeout reached.")
			if *shutdownFlag {
				return triggerShutdownCountdown(ctx)
			}
			return nil
		case <-tickerChan:
			elapsed := time.Since(startTime)
			hours := int(elapsed.Hours())
			minutes := int(elapsed.Minutes()) % 60
			seconds := int(elapsed.Seconds()) % 60

			writeElapsed(hours, minutes, seconds)
		}
	}
}

func triggerShutdownCountdown(ctx context.Context) error {
	fmt.Println("\nWARNING: Shutdown triggered. Press Ctrl+C to cancel.")

	useTicker := isTerminal(os.Stdout)
	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	for i := 60; i > 0; i-- {
		select {
		case <-ctx.Done():
			if useTicker {
				fmt.Println()
			}
			fmt.Println("Shutdown cancelled.")
			return nil
		case <-ticker.C:
			if useTicker {
				writeShutdownCountdown(i)
			} else {
				if i%10 == 0 || i <= 5 {
					fmt.Printf("Shutting down in %d seconds...\n", i)
				}
			}
		}
	}

	if useTicker {
		fmt.Println()
	}
	fmt.Println("Shutting down now...")
	var cmd *exec.Cmd
	if runtime.GOOS == "windows" {
		cmd = exec.Command("shutdown", "/s", "/t", "0")
	} else {
		cmd = exec.Command("shutdown", "-h", "now")
	}

	if err := cmd.Run(); err != nil {
		return fmt.Errorf("failed to execute system shutdown (ensure you have administrative/sudo privileges): %w", err)
	}
	return nil
}

func writeElapsed(hours, minutes, seconds int) {
	var buf [64]byte
	const prefix = "\rElapsed: "
	copy(buf[:], prefix)
	idx := len(prefix)

	idx = appendInt(buf[:], idx, hours)

	buf[idx] = ':'
	idx++

	idx = appendInt2(buf[:], idx, minutes)

	buf[idx] = ':'
	idx++

	idx = appendInt2(buf[:], idx, seconds)

	const suffix = "\033[K"
	copy(buf[idx:], suffix)
	idx += len(suffix)

	_, _ = os.Stdout.Write(buf[:idx])
}

func writeShutdownCountdown(seconds int) {
	var buf [64]byte
	const prefix = "\rShutting down in "
	copy(buf[:], prefix)
	idx := len(prefix)

	idx = appendIntRaw(buf[:], idx, seconds)

	const suffix = " seconds...\033[K"
	copy(buf[idx:], suffix)
	idx += len(suffix)

	_, _ = os.Stdout.Write(buf[:idx])
}

func appendInt(buf []byte, idx int, val int) int {
	if val == 0 {
		buf[idx] = '0'
		buf[idx+1] = '0'
		return idx + 2
	}
	if val < 10 {
		buf[idx] = '0'
		buf[idx+1] = byte('0' + val)
		return idx + 2
	}
	var digits [16]byte
	dIdx := 0
	for val > 0 {
		digits[dIdx] = byte('0' + val%10)
		dIdx++
		val /= 10
	}
	for i := dIdx - 1; i >= 0; i-- {
		buf[idx] = digits[i]
		idx++
	}
	return idx
}

func appendInt2(buf []byte, idx int, val int) int {
	buf[idx] = byte('0' + val/10)
	buf[idx+1] = byte('0' + val%10)
	return idx + 2
}

func appendIntRaw(buf []byte, idx int, val int) int {
	if val == 0 {
		buf[idx] = '0'
		return idx + 1
	}
	var digits [16]byte
	dIdx := 0
	for val > 0 {
		digits[dIdx] = byte('0' + val%10)
		dIdx++
		val /= 10
	}
	for i := dIdx - 1; i >= 0; i-- {
		buf[idx] = digits[i]
		idx++
	}
	return idx
}
