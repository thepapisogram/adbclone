#!/bin/sh
# Everything that must pass before a commit: Rust tests, type check, unit tests, and the browser regression suite.
cd "$(dirname "$0")/.." || exit 2
echo "== no source file is hidden from git"
# A file the build needs but git ignores passes locally and fails on a clean checkout (this has happened).
hidden=$(git ls-files --others --ignored --exclude-standard -- ui/src src-tauri/src src-tauri/data src-tauri/capabilities adb-core/src adb-core/tests helper ui/e2e brand/*.cjs brand/*.svg scripts tools 2>/dev/null | grep -v "/target/\|node_modules")
if [ -n "$hidden" ]; then echo "These files are ignored by git but look like source:"; echo "$hidden"; exit 1; fi
echo "== cargo test"
cargo test --workspace >/tmp/check-cargo.log 2>&1 || { tail -30 /tmp/check-cargo.log; echo "CARGO TESTS FAILED"; exit 1; }
echo "== windows compile check"; cargo check -p adb-core --target x86_64-pc-windows-gnu >/tmp/check-win.log 2>&1 || { tail -20 /tmp/check-win.log; echo "WINDOWS CHECK FAILED"; exit 1; }
echo "== tsc + vitest"; (cd ui && npx tsc -b && npx vitest run 2>&1 | tail -4) || { echo "UI CHECKS FAILED"; exit 1; }
echo "== browser regression suite"; ui/e2e/run.sh || { echo "E2E FAILED"; exit 1; }
echo "== a clean copy of what git would contain builds"
clean=$(mktemp -d)
git ls-files -co --exclude-standard | while read -r f; do [ -e "$f" ] && echo "$f"; done | tar cf - -T - | (cd "$clean" && tar xf -)
(cd "$clean/ui" && npm ci --silent >/dev/null 2>&1 && npm run build --silent >/dev/null 2>&1) || { echo "CLEAN COPY: UI DOES NOT BUILD"; rm -rf "$clean"; exit 1; }
(cd "$clean" && CARGO_TARGET_DIR="$PWD/../coolant-clean-target" cargo check --workspace >/tmp/check-clean.log 2>&1) || { tail -20 /tmp/check-clean.log; echo "CLEAN COPY: RUST DOES NOT BUILD"; rm -rf "$clean"; exit 1; }
rm -rf "$clean"
echo "ALL CHECKS PASSED"
