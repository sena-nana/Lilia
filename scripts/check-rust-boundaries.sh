#!/usr/bin/env bash
# Forbidden dependency edges for Lilia product crates (#53).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail=0

check_no_match() {
  local label="$1"
  local path="$2"
  local pattern="$3"
  if rg -n --glob '!target/**' "$pattern" "$path" >/dev/null 2>&1; then
    echo "FAIL: $label — found `$pattern` under $path"
    rg -n --glob '!target/**' "$pattern" "$path" || true
    fail=1
  else
    echo "OK: $label"
  fi
}

check_no_match "lilia-contracts must not depend on tauri/vue" \
  "crates/lilia-contracts" 'tauri|vue|android|vscode'

check_no_match "lilia-core must not depend on tauri/vue/host UI" \
  "crates/lilia-core" 'tauri|vue|android|vscode'

check_no_match "lilia-core must not depend on concrete Model Adapter crates" \
  "crates/lilia-core" 'mutsuki-agent-adapter|mutsuki-agent-bundle|mutsuki-agent-client'

check_no_match "lilia-storage must not depend on tauri/vue/android/vscode" \
  "crates/lilia-storage" 'tauri|vue|android|vscode'

check_no_match "lilia-agent-integration must not depend on tauri/vue" \
  "crates/lilia-agent-integration" 'tauri|vue'

check_no_match "lilia-client must not depend on tauri" \
  "crates/lilia-client" 'tauri'

check_no_match "lilia-service must not depend on tauri/vue" \
  "crates/lilia-service" 'tauri|vue'

check_no_match "apps/service must not depend on tauri/vue" \
  "apps/service" 'tauri|vue'

check_no_match "apps/cli must not depend on tauri/vue" \
  "apps/cli" 'tauri|vue'

if [[ ! -f Cargo.toml ]]; then
  echo "FAIL: root Cargo.toml missing"
  fail=1
else
  echo "OK: root Cargo.toml present"
fi

if [[ ! -d crates/lilia-contracts || ! -d crates/lilia-core || ! -d crates/lilia-storage || ! -d crates/lilia-service || ! -d apps/service || ! -d apps/cli ]]; then
  echo "FAIL: minimal core crates / apps/service / apps/cli missing"
  fail=1
else
  echo "OK: minimal core crates present"
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi

echo "Dependency boundary check passed."
