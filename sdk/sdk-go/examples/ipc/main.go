// Subscribe to shell events and dispatch an action.
//
// Run against a live Glimpse session:
//
//	go run ./examples/ipc
package main

import (
	"context"
	"fmt"
	"log"

	"github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"
)

func main() {
	ctx := context.Background()

	// Cheap: resolves the socket path, no connection yet.
	sub := sdk.IPC("shell")

	// One-shot connection; awaits the ack. Errors if the server rejects it.
	ack, err := sub.Dispatch(ctx, "open_uri", map[string]string{
		"uri": "https://example.com",
	})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("dispatch ack:", ack)

	// Channel of events; closed when the socket closes.
	events, err := sub.Listen(ctx, "audio.*")
	if err != nil {
		log.Fatal(err)
	}
	for ev := range events {
		fmt.Println(ev.Name, ev.Ts, ev.Fields)
	}
	if err := sub.Err(); err != nil {
		log.Print(err)
	}
}
