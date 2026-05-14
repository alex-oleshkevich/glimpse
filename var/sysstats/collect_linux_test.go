package main

import (
	"strings"
	"testing"
)

func TestParseMeminfoComputesPercentages(t *testing.T) {
	stats, swap, err := parseMeminfo(strings.NewReader(sampleMeminfo))
	if err != nil {
		t.Fatalf("parse meminfo: %v", err)
	}
	if stats.TotalBytes != 16*gib {
		t.Fatalf("expected 16 GiB total RAM, got %d", stats.TotalBytes)
	}
	if stats.UsedPercent < 60 || stats.UsedPercent > 80 {
		t.Fatalf("expected RAM usage in realistic range, got %f", stats.UsedPercent)
	}
	if swap.UsedPercent != 50 {
		t.Fatalf("expected 50%% swap usage, got %f", swap.UsedPercent)
	}
}

func TestParseNvidiaSmiCSVHandlesSingleGpu(t *testing.T) {
	gpus, err := parseNvidiaSMI(strings.NewReader("0, NVIDIA GeForce RTX 3070, 12, 1024, 8192, 54, Enabled\n"))
	if err != nil {
		t.Fatalf("parse nvidia-smi: %v", err)
	}
	if len(gpus) != 1 {
		t.Fatalf("expected 1 gpu, got %d", len(gpus))
	}
	if gpus[0].Vendor != "NVIDIA" {
		t.Fatalf("expected NVIDIA vendor, got %q", gpus[0].Vendor)
	}
	if gpus[0].UtilPercent != 12 {
		t.Fatalf("expected utilization 12, got %f", gpus[0].UtilPercent)
	}
	if !gpus[0].Powered {
		t.Fatal("expected powered gpu")
	}
}

const sampleMeminfo = `MemTotal:       16777216 kB
MemFree:         2097152 kB
MemAvailable:    5242880 kB
Buffers:          262144 kB
Cached:          3145728 kB
SwapTotal:       8388608 kB
SwapFree:        4194304 kB
`
