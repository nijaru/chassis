#!/usr/bin/env bash
# Build the pinned validator with reviewed event-generator corrections.
set -euo pipefail

revision=${1:?usage: install-clap-validator.sh PINNED_REVISION}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source_dir=$(mktemp -d)
trap 'rm -rf "$source_dir"' EXIT

git -C "$source_dir" init --quiet
git -C "$source_dir" remote add origin https://github.com/free-audio/clap-validator
git -C "$source_dir" -c core.autocrlf=false fetch --quiet --depth=1 origin "$revision"
git -C "$source_dir" -c core.autocrlf=false checkout --quiet --detach FETCH_HEAD
git -C "$source_dir" apply "$root/vendor/clap-validator/global-modulation.patch"
git -C "$source_dir" apply "$root/vendor/clap-validator/native-note-ranges.patch"

# Keep tool artifacts isolated from the workspace and the user's installed CLI.
export CARGO_TARGET_DIR="$root/target/clap-validator-build"
cargo +1.98.0 test --release --locked --manifest-path "$source_dir/Cargo.toml" \
    -p clap-validator --bin clap-validator
cargo +1.98.0 install --locked --path "$source_dir" \
    --root "$root/target/clap-validator" --force clap-validator
