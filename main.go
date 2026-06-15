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

func printHelp() {
	fmt.Print(`vigil — keep your system awake

USAGE
  vigil <command> [flags]

COMMANDS
  start   Start the sleep inhibitor
  help    Show this help message

Run 'vigil <command> -h' for command-specific help.

EXAMPLES
  vigil start               Keep awake indefinitely (Ctrl+C to stop)
  vigil start -t 2h         Keep awake for 2 hours, then exit
  vigil start -t 45m -s     Keep awake for 45 minutes, then shut down

`)
}

func run(args []string) error {
	if len(args) == 0 {
		printHelp()
		return nil
	}

	switch args[0] {
	case "start":
		return cmdStart(args[1:])
	case "help", "-h", "--help":
		printHelp()
		return nil
	default:
		fmt.Fprintf(os.Stderr, "vigil: unknown command %q\n\n", args[0])
		printHelp()
		os.Exit(1)
	}
	return nil
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
