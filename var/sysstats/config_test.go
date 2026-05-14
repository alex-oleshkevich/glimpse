package main

import "testing"

func TestLoadConfigMergesDefaultsAndFile(t *testing.T) {
	cfg, err := LoadConfig(ConfigEnv{
		ExplicitPath: "testdata/sysstats.toml",
	})
	if err != nil {
		t.Fatalf("load config: %v", err)
	}

	if cfg.Panel.RefreshMS != 1500 {
		t.Fatalf("expected refresh override, got %d", cfg.Panel.RefreshMS)
	}
	if cfg.Panel.Format != "{cpu} | {ram} | {swap}" {
		t.Fatalf("expected custom panel format, got %q", cfg.Panel.Format)
	}
	if cfg.Panel.Items.CPU.Label != "PROC" {
		t.Fatalf("expected CPU label override, got %q", cfg.Panel.Items.CPU.Label)
	}
	if cfg.Panel.Items.RAM.Icon != "computer-symbolic" {
		t.Fatalf("expected default RAM icon, got %q", cfg.Panel.Items.RAM.Icon)
	}
	if cfg.Panel.Items.Swap.WarnAt != 85 {
		t.Fatalf("expected swap warn override, got %d", cfg.Panel.Items.Swap.WarnAt)
	}
	if cfg.Popover.Sections.Network {
		t.Fatal("expected network section override to disable network")
	}
	if !cfg.Popover.Sections.Uptime {
		t.Fatal("expected uptime section to remain enabled by default")
	}
}
