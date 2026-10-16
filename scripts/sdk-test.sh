#!/usr/bin/env bash
set -euo pipefail

cd sdk/sdk-ts
timeout 300 npm ci
timeout 300 npm run check
timeout 300 node --test
