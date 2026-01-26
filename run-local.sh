#!/bin/bash
cd "$(dirname "$0")/src/local-daemon"
cargo run --release
