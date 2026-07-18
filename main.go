package main

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"strings"
	"syscall"
	"time"

	"github.com/alecthomas/kong"
)

var version = "dev"

// CLI is the root command structure parsed by kong.
type CLI struct {
	Start   StartCmd   `cmd:"" help:"Start the sleep inhibitor"`
	Version VersionCmd `cmd:"" help:"Show version information"`
}

func main() {
	// Resolve the binary name the same way min does.
	appName := filepath.Base(os.Args[0])
	appName = strings.TrimSuffix(appName, filepath.Ext(appName))
	if appName == "" || appName == "main" || strings.HasPrefix(appName, "go-build") || strings.HasSuffix(appName, ".test") {
		appName = "vigil"
	}

	cli := &CLI{}
	ctx, err := kong.New(cli,
		kong.Name(appName),
		kong.Description("Keep your system awake."),
		kong.UsageOnError(),
		kong.ConfigureHelp(kong.HelpOptions{
			Compact: true,
			Tree:    true,
		}),
	)
	if err != nil {
		fmt.Fprintln(os.Stderr, "Error:", err)
		os.Exit(1)
	}

	kctx, err := ctx.Parse(os.Args[1:])
	ctx.FatalIfErrorf(err)

	// Inject a cancellable context that is cancelled on SIGINT/SIGTERM.
	sigCtx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	kctx.BindTo(sigCtx, (*context.Context)(nil))

	kctx.FatalIfErrorf(kctx.Run())
}

// ---------------------------------------------------------------------------
// start subcommand
// ---------------------------------------------------------------------------

// StartCmd starts the sleep inhibitor.
type StartCmd struct {
	Timeout  time.Duration `short:"t" help:"Stay awake for this duration, then exit (e.g. 2h, 45m, 30s). Omit to run indefinitely." placeholder:"DURATION"`
	Shutdown bool          `short:"s" help:"Shut down the system when the timeout expires. Requires -t."`
}

func (cmd *StartCmd) Validate() error {
	if cmd.Shutdown && cmd.Timeout == 0 {
		return fmt.Errorf("cannot use -s (shutdown) without specifying a timeout duration via -t")
	}
	if cmd.Timeout < 0 {
		return fmt.Errorf("timeout duration must be positive")
	}
	return nil
}

func (cmd *StartCmd) Run(ctx context.Context) error {
	stopInhibit, err := startInhibitFn()
	if err != nil {
		return fmt.Errorf("failed to initialize sleep inhibitor: %w", err)
	}
	defer stopInhibit()

	startTime := time.Now()
	var timeoutChan <-chan time.Time

	if cmd.Timeout > 0 {
		timeoutChan = time.After(cmd.Timeout)
		stopTime := startTime.Add(cmd.Timeout).Format(time.DateTime)
		if cmd.Shutdown {
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
		ticker = time.NewTicker(time.Second)
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
			if cmd.Shutdown {
				return triggerShutdownCountdown(ctx)
			}
			return nil
		case <-tickerChan:
			elapsed := time.Since(startTime)
			writeElapsed(int(elapsed.Hours()), int(elapsed.Minutes())%60, int(elapsed.Seconds())%60)
		}
	}
}

// ---------------------------------------------------------------------------
// version subcommand
// ---------------------------------------------------------------------------

// VersionCmd prints the build version.
type VersionCmd struct{}

func (cmd *VersionCmd) Run() error {
	fmt.Printf("vigil %s\n", version)
	return nil
}

// ---------------------------------------------------------------------------
// helpers (unchanged)
// ---------------------------------------------------------------------------

var startInhibitFn = startInhibit

func isTerminal(f *os.File) bool {
	stat, err := f.Stat()
	if err != nil {
		return false
	}
	return (stat.Mode() & os.ModeCharDevice) != 0
}

func triggerShutdownCountdown(ctx context.Context) error {
	fmt.Println("\nWARNING: Shutdown triggered. Press Ctrl+C to cancel.")

	useTicker := isTerminal(os.Stdout)
	ticker := time.NewTicker(time.Second)
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

// Zero-allocation progress formatters.

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
