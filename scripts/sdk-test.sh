#!/usr/bin/env bash
set -euo pipefail

(
  cd sdk/sdk-ts
  timeout 300 npm ci
  timeout 300 npm run check
  timeout 300 node --test
)

(cd sdk/sdk-py && timeout 300 python3 -m unittest discover -s tests)
(cd sdk/sdk-go && timeout 300 go test ./...)
(cd sdk/sdk-rs && timeout 600 cargo test)
(cd sdk/sdk-rs/examples/kitchen_sink && timeout 600 cargo build)
