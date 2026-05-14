package main

import (
	"context"
	"sync"
	"time"

	sdk "github.com/glimpse-project/custom-applet-sdk-go/sdk"
)

type sysstatsApplet struct {
	sdk.BaseApplet[struct{}]

	mu        sync.RWMutex
	snapshot  Snapshot
	config    Config
	collector Collector
}

func newSysstatsApplet(config Config, collector Collector) *sysstatsApplet {
	return &sysstatsApplet{
		BaseApplet: sdk.NewBaseApplet(struct{}{}),
		config:     config,
		collector:  collector,
	}
}

func newTestApplet() *sysstatsApplet {
	return newSysstatsApplet(DefaultConfig(), noopCollector{})
}

func (a *sysstatsApplet) OnStart(ctx context.Context) error {
	if err := a.refresh(ctx); err != nil {
		return err
	}

	go a.poll(ctx)
	return nil
}

func (a *sysstatsApplet) OnInit(context.Context, sdk.InitEvent) error { return nil }
func (a *sysstatsApplet) OnCallback(context.Context, sdk.CallbackEvent) error {
	return nil
}

func (a *sysstatsApplet) Render(context.Context) (sdk.RenderResult, error) {
	snapshot := a.currentSnapshot()
	return sdk.RenderResult{
		Status: buildStatusItems(a.config, snapshot),
		Tree:   buildPopoverTree(a.config, snapshot),
	}, nil
}

func (a *sysstatsApplet) poll(ctx context.Context) {
	ticker := time.NewTicker(time.Duration(a.config.Panel.RefreshMS) * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			_ = a.refresh(ctx)
		}
	}
}

func (a *sysstatsApplet) refresh(ctx context.Context) error {
	snapshot, err := a.collector.Collect(ctx)
	if err != nil {
		return err
	}
	a.setSnapshot(snapshot)
	return nil
}

func (a *sysstatsApplet) currentSnapshot() Snapshot {
	a.mu.RLock()
	defer a.mu.RUnlock()
	return a.snapshot
}

func (a *sysstatsApplet) setSnapshot(snapshot Snapshot) {
	a.mu.Lock()
	a.snapshot = snapshot
	a.mu.Unlock()
	a.SetState(func(*struct{}) {})
}

type noopCollector struct{}

func (noopCollector) Collect(context.Context) (Snapshot, error) {
	return Snapshot{}, nil
}
