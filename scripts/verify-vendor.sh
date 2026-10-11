#!/usr/bin/env bash
# Verifies that the vendored libexpat files are byte-identical to the pinned
# upstream commit (hashes recorded when vendoring, see NOTICE).
set -euo pipefail
cd "$(dirname "$0")/../crates/expat-oracle/vendor"
sha256sum --quiet -c ../vendor.sha256 && echo "verify-vendor: OK ($(wc -l < ../vendor.sha256) files)"
