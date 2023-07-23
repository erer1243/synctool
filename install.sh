#!/usr/bin/env bash
set -ex
cd "$(dirname "$0")"
cargo build --release
cp target/release/synctool ~/prog/sync
strip ~/prog/sync
