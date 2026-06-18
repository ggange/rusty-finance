#!/usr/bin/env bash
# scripts/dev.sh — start API + Vite dev server with health gate.
# Called by `make dev` after bindings are already built.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENV="$REPO_ROOT/.venv"

# --- guard: venv must exist ---
if [ ! -f "$VENV/bin/python" ]; then
  echo "ERROR: .venv not found. Run 'make setup' first."
  exit 1
fi

# --- cleanup on exit (Ctrl+C, error, or normal exit) ---
API_PID=""
cleanup() {
  echo ""
  echo "→ Shutting down..."
  if [ -n "$API_PID" ] && kill -0 "$API_PID" 2>/dev/null; then
    kill "$API_PID"
  fi
  # Belt-and-suspenders: release port 8000 if anything else snuck in
  local stray
  stray=$(lsof -ti:8000 2>/dev/null || true)
  if [ -n "$stray" ]; then
    kill "$stray" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

# --- start FastAPI backend ---
echo "→ Starting FastAPI backend..."
# Use .venv/bin/uvicorn directly so the right Python is guaranteed
cd "$REPO_ROOT"
"$VENV/bin/uvicorn" api.main:app --reload &
API_PID=$!

# --- health gate: wait for engine:available (up to 20 s) ---
echo "→ Waiting for backend to be ready..."
ATTEMPTS=0
MAX_ATTEMPTS=40
until curl -sf http://localhost:8000/health 2>/dev/null | grep -q '"engine":"available"'; do
  ATTEMPTS=$((ATTEMPTS + 1))
  if [ "$ATTEMPTS" -ge "$MAX_ATTEMPTS" ]; then
    echo ""
    echo "ERROR: Backend did not become ready after 20 s."
    echo "  Check: $VENV/bin/uvicorn is using Python from .venv"
    echo "  Check: cd backtesting-py && make bindings"
    echo "  Check: curl http://localhost:8000/health"
    exit 1
  fi
  # Show progress without a newline
  if [ $((ATTEMPTS % 4)) -eq 0 ]; then
    printf "."
  fi
  sleep 0.5
done
echo ""
echo "✓ Backend ready — http://localhost:8000  (docs: /docs)"

# --- start Vite dev server (foreground; owns the terminal) ---
echo "→ Starting Vite dev server..."
echo ""
cd "$REPO_ROOT/frontend"
exec npm run dev
