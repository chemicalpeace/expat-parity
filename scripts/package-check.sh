#!/usr/bin/env bash
# Checks the published crate, `crates/expat`: its LICENSE and NOTICE are
# byte-identical to the repository's, the package holds them and the
# README, it builds from the package (`cargo package` verifies), and its
# own tests compile from the package alone, outside the workspace.
set -euo pipefail
cd "$(dirname "$0")/.."
cmp LICENSE crates/expat/LICENSE
cmp NOTICE crates/expat/NOTICE
list=$(cargo package -p expat --list --allow-dirty)
for f in Cargo.toml LICENSE NOTICE README.md src/lib.rs tests/public_api.rs; do
  grep -qx "$f" <<<"$list" || { echo "package-check: $f missing from the package"; exit 1; }
done
if grep -q '^\.\./' <<<"$list"; then echo "package-check: path outside the package"; exit 1; fi
cargo package -p expat --allow-dirty
target=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
version=$(cargo pkgid -p expat | sed 's/.*[@#]//')
toolchain=$(rustup show active-toolchain | cut -d' ' -f1)
tmp=$(mktemp -d)
tar -xzf "$target/package/expat-$version.crate" -C "$tmp"
(cd "$tmp/expat-$version" && RUSTUP_TOOLCHAIN="$toolchain" CARGO_TARGET_DIR="$tmp/target" cargo test --no-run)
rm -rf "$tmp"
echo "package-check: OK (expat $version)"
