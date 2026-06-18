package main

import (
	"strings"
	"testing"
)

func TestRunCommandParsing(t *testing.T) {
	// Mock the inhibitor start function to avoid running OS commands
	oldFn := startInhibitFn
	defer func() { startInhibitFn = oldFn }()
	startInhibitFn = func() (func(), error) {
		return func() {}, nil
	}

	tests := []struct {
		name        string
		args        []string
		wantErr     bool
		errContains string
	}{
		{
			name:    "empty args prints help and returns nil",
			args:    []string{},
			wantErr: false,
		},
		{
			name:    "version command prints version",
			args:    []string{"version"},
			wantErr: false,
		},
		{
			name:    "version short flag",
			args:    []string{"-v"},
			wantErr: false,
		},
		{
			name:    "version long flag",
			args:    []string{"--version"},
			wantErr: false,
		},
		{
			name:    "help command",
			args:    []string{"help"},
			wantErr: false,
		},
		{
			name:        "invalid command returns error",
			args:        []string{"nonexistent"},
			wantErr:     true,
			errContains: `unknown command "nonexistent"`,
		},
		{
			name:        "start with shutdown but no timeout returns error",
			args:        []string{"start", "-s"},
			wantErr:     true,
			errContains: "cannot use -s (shutdown) without specifying a timeout",
		},
		{
			name:        "start with invalid timeout returns error",
			args:        []string{"start", "-t", "invalid"},
			wantErr:     true,
			errContains: "invalid timeout",
		},
		{
			name:        "start with non-positive timeout returns error",
			args:        []string{"start", "-t", "-10s"},
			wantErr:     true,
			errContains: "timeout duration must be positive",
		},
		{
			name:        "start with zero timeout returns error",
			args:        []string{"start", "-t", "0s"},
			wantErr:     true,
			errContains: "timeout duration must be positive",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := run(tt.args)
			if (err != nil) != tt.wantErr {
				t.Fatalf("run(%v) error = %v, wantErr = %v", tt.args, err, tt.wantErr)
			}
			if err != nil && tt.errContains != "" {
				if !strings.Contains(err.Error(), tt.errContains) {
					t.Errorf("expected error containing %q, got %q", tt.errContains, err.Error())
				}
			}
		})
	}
}
