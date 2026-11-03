#!/bin/sh
# Regression suite: builds the UI, serves it, and drives it with Playwright against the mock backend.
# Needs Playwright installed globally (NODE_PATH is taken from `npm root -g`). Exit code 0 = all passed.
cd "$(dirname "$0")/.." || exit 2
npm run build >/dev/null 2>&1 || { echo "UI build failed"; exit 1; }
PORT=${PORT:-4199}
npx vite preview --port "$PORT" >/dev/null 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null' EXIT
sleep 2
fail=0
for f in e2e/*.cjs; do
  out=$(BASE="http://localhost:$PORT" NODE_PATH="$(npm root -g)" timeout 150 node "$f" /tmp/e2e-shot-1.png /tmp/e2e-shot-2.png 2>&1)
  status=$?
  # A script fails if it throws, prints console errors, or prints a check that came out false.
  bad=$(echo "$out" | grep -E ': false|errors \[ [^]]|Error:|TimeoutError' | head -3)
  if [ $status -ne 0 ] || [ -n "$bad" ]; then
    fail=1; echo "FAIL  $f"; echo "$out" | tail -n 12
  else
    echo "ok    $f"
  fi
done
exit $fail
