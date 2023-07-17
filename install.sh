#!/usr/bin/env bash
set -ev
cargo build --release
cp target/release/synctool ~/prog/sync
strip ~/prog/sync
