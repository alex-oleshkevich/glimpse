package main

import (
	"context"
	"log"

	sdk "github.com/glimpse-project/custom-applet-sdk-go/sdk"
)

func main() {
	config, err := LoadConfig(ConfigEnv{})
	if err != nil {
		log.Fatalf("load sysstats config: %v", err)
	}

	applet := newSysstatsApplet(config, newLinuxCollector())
	if err := sdk.Run[struct{}](context.Background(), applet); err != nil {
		log.Fatal(err)
	}
}
