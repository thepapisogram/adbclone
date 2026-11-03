#!/bin/sh
# Everything that must pass before a commit: Rust tests, type check, unit tests, and the browser regression suite.
cd "$(dirname "$0")/.." || exit 2
echo "== cargo test"
cargo test --workspace >/tmp/check-cargo.log 2>&1 || { tail -30 /tmp/check-cargo.log; echo "CARGO TESTS FAILED"; exit 1; }
echo "== windows compile check"; cargo check -p adb-core --target x86_64-pc-windows-gnu >/tmp/check-win.log 2>&1 || { tail -20 /tmp/check-win.log; echo "WINDOWS CHECK FAILED"; exit 1; }
echo "== tsc + vitest"; (cd ui && npx tsc -b && npx vitest run 2>&1 | tail -4) || { echo "UI CHECKS FAILED"; exit 1; }
echo "== browser regression suite"; ui/e2e/run.sh || { echo "E2E FAILED"; exit 1; }
echo "ALL CHECKS PASSED"
