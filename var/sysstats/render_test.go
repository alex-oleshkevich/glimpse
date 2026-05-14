package main

import (
	"context"
	"testing"
	"time"

	sdk "github.com/glimpse-project/custom-applet-sdk-go/sdk"
)

func TestRenderUsesConfiguredPanelIconsAndLabels(t *testing.T) {
	app := newTestApplet()
	app.snapshot = Snapshot{
		CPUPercent: 12.4,
		Memory: MemoryStats{
			UsedPercent: 67.2,
			UsedBytes:   11 * gib,
			TotalBytes:  16 * gib,
		},
		Swap: SwapStats{
			UsedPercent: 4.1,
			UsedBytes:   gib / 2,
			TotalBytes:  8 * gib,
		},
		Load:         LoadStats{One: 0.42, Five: 0.55, Fifteen: 0.61},
		Uptime:       2*time.Hour + 3*time.Minute,
		UpdatedAt:    time.Date(2026, 4, 8, 13, 21, 4, 0, time.UTC),
		Network:      NetworkStats{Interface: "enp5s0", State: "up", IPv4: "192.168.1.24"},
		Temperatures: []TemperatureReading{{Label: "CPU", Celsius: 61}},
	}

	app.config.Panel.Items.CPU.Icon = "computer-symbolic"
	app.config.Panel.Items.CPU.Label = "CPU"
	app.config.Panel.Items.RAM.Icon = "computer-symbolic"
	app.config.Panel.Items.RAM.Label = "RAM"
	app.config.Panel.Items.Swap.Icon = "drive-harddisk-symbolic"
	app.config.Panel.Items.Swap.Label = "SWP"

	rendered, err := app.Render(context.Background())
	if err != nil {
		t.Fatalf("render: %v", err)
	}
	if len(rendered.Status) != 3 {
		t.Fatalf("expected 3 status items, got %d", len(rendered.Status))
	}
	if rendered.Status[0].Label != "CPU 12%" {
		t.Fatalf("expected formatted CPU status, got %q", rendered.Status[0].Label)
	}
	if rendered.Status[1].Label != "RAM 67%" {
		t.Fatalf("expected formatted RAM status, got %q", rendered.Status[1].Label)
	}
	if rendered.Status[2].Label != "SWP 4%" {
		t.Fatalf("expected formatted swap status, got %q", rendered.Status[2].Label)
	}
	if rendered.Tree == nil {
		t.Fatal("expected popover tree")
	}
	root, ok := rendered.Tree.Data.(sdk.Box)
	if !ok {
		t.Fatalf("expected box tree, got %T", rendered.Tree.Data)
	}
	if len(root.Children) == 0 || root.Children[0].Type != "hero" {
		t.Fatalf("expected hero as first popover child, got %#v", root.Children)
	}
}

func TestBuildPopoverTreeUsesSharedSectionAndPropertyListComponents(t *testing.T) {
	config := DefaultConfig()
	snapshot := Snapshot{
		CPUPercent: 42.1,
		Memory: MemoryStats{
			UsedPercent: 67.2,
			UsedBytes:   11 * gib,
			TotalBytes:  16 * gib,
		},
		Swap: SwapStats{
			UsedPercent: 4.1,
			UsedBytes:   gib / 2,
			TotalBytes:  8 * gib,
		},
		Load:      LoadStats{One: 0.42, Five: 0.55, Fifteen: 0.61},
		Uptime:    2*time.Hour + 3*time.Minute,
		UpdatedAt: time.Date(2026, 4, 8, 13, 21, 4, 0, time.UTC),
		Network: NetworkStats{
			Interface:     "enp5s0",
			State:         "up",
			IPv4:          "192.168.1.24",
			RXBytesPerSec: 1024,
			TXBytesPerSec: 2048,
		},
		GPUs: []GPUStats{{
			Name:             "RTX",
			Vendor:           "NVIDIA",
			UtilPercent:      48,
			MemoryUsedBytes:  2 * gib,
			MemoryTotalBytes: 8 * gib,
			TempCelsius:      68,
		}},
		Disks: []DiskStats{{
			Device:     "/dev/nvme0n1p2",
			Mount:      "/",
			UsedBytes:  120 * gib,
			TotalBytes: 256 * gib,
		}},
		Temperatures: []TemperatureReading{{Label: "CPU", Celsius: 61}},
	}

	tree := buildPopoverTree(config, snapshot)
	if tree == nil {
		t.Fatal("expected popover tree")
	}

	root, ok := tree.Data.(sdk.Box)
	if !ok {
		t.Fatalf("expected root box, got %T", tree.Data)
	}
	if len(root.Children) != 7 {
		t.Fatalf("expected hero plus 6 section children, got %d", len(root.Children))
	}
	if root.Children[0].Type != "hero" {
		t.Fatalf("expected first child to be hero, got %q", root.Children[0].Type)
	}

	expectedTitles := []string{
		"CPU & Memory",
		"Network",
		"GPU",
		"Disk",
		"Temperatures",
		"Uptime",
	}

	for idx, child := range root.Children[1:] {
		if child.Type != "section" {
			t.Fatalf("expected section child at %d, got %q", idx, child.Type)
		}
		section, ok := child.Data.(sdk.Section)
		if !ok {
			t.Fatalf("expected section data at %d, got %T", idx, child.Data)
		}
		if section.Title != expectedTitles[idx] {
			t.Fatalf("expected section title %q at %d, got %q", expectedTitles[idx], idx, section.Title)
		}
		if len(section.Children) != 1 {
			t.Fatalf("expected one semantic child in section %q, got %d", section.Title, len(section.Children))
		}
		if section.Children[0].Type != "property_list" {
			t.Fatalf("expected property_list in section %q, got %q", section.Title, section.Children[0].Type)
		}
	}
}

func TestBuildPopoverTreeShowsNetworkEmptyStateWhenNoNetworkData(t *testing.T) {
	config := DefaultConfig()
	config.Popover.Sections.GPU = false
	config.Popover.Sections.Disk = false
	config.Popover.Sections.Temps = false
	config.Popover.Sections.Uptime = false

	tree := buildPopoverTree(config, Snapshot{})
	if tree == nil {
		t.Fatal("expected popover tree")
	}

	root, ok := tree.Data.(sdk.Box)
	if !ok {
		t.Fatalf("expected root box, got %T", tree.Data)
	}
	if len(root.Children) != 3 {
		t.Fatalf("expected hero, CPU, and Network sections, got %d", len(root.Children))
	}

	network, ok := root.Children[2].Data.(sdk.Section)
	if !ok {
		t.Fatalf("expected network section, got %T", root.Children[2].Data)
	}
	if network.Title != "Network" {
		t.Fatalf("expected network section title, got %q", network.Title)
	}
	if len(network.Children) != 1 {
		t.Fatalf("expected one child in network section, got %d", len(network.Children))
	}
	if network.Children[0].Type != "empty_state" {
		t.Fatalf("expected empty_state for missing network data, got %q", network.Children[0].Type)
	}
}
