VENV   := $(CURDIR)/.venv
PYTHON := $(VENV)/bin/python
UVICORN := $(VENV)/bin/uvicorn
MATURIN := $(VENV)/bin/maturin
PIP    := $(VENV)/bin/pip

TICKER ?= AAPL
START  ?= 2020-01-01
END    ?= 2024-12-31

SYMBOLS ?= AAPL MSFT GOOG SPY NVDA

.PHONY: setup bindings dev test build fetch fetch-all refresh help

help:
	@echo "Usage:"
	@echo "  make setup          Create .venv and install all dependencies (run once)"
	@echo "  make bindings       Rebuild Rust→Python bindings (run after Rust changes)"
	@echo "  make dev            Build bindings + start API + Vite dev server"
	@echo "  make test           Run all tests: cargo, pytest, tsc+vite build"
	@echo "  make build          Production frontend build only"
	@echo "  make fetch          Fetch real OHLCV for one ticker (TICKER=AAPL)"
	@echo "  make fetch-all      Fetch AAPL MSFT GOOG SPY NVDA for 2020-2024"
	@echo "  make refresh        Append new bars to existing datasets (SYMBOLS=...)"

setup:
	@echo "→ Creating Python virtual environment..."
	python3 -m venv $(VENV)
	$(PIP) install --upgrade pip maturin
	$(PIP) install -e "api/[dev]"
	@echo "→ Installing frontend dependencies..."
	cd frontend && npm install
	@echo ""
	@echo "✓ Setup complete. Run 'make dev' to start the app."

bindings:
	@if [ ! -f "$(MATURIN)" ]; then echo "ERROR: .venv not found. Run 'make setup' first."; exit 1; fi
	@echo "→ Building Rust bindings..."
	cd backtesting-py && $(MATURIN) develop

dev: bindings
	@bash scripts/dev.sh

test:
	@echo "→ Rust tests..."
	cargo test -p backtesting
	@echo ""
	@echo "→ Python/API tests..."
	@if [ ! -f "$(PYTHON)" ]; then echo "ERROR: .venv not found. Run 'make setup' first."; exit 1; fi
	cd backtesting-py && $(MATURIN) develop --quiet
	$(VENV)/bin/pytest api/tests/ -v
	@echo ""
	@echo "→ Frontend type-check + build..."
	cd frontend && npm run build

build:
	cd frontend && npm run build

fetch:
	@if [ ! -f "$(PYTHON)" ]; then echo "ERROR: .venv not found. Run 'make setup' first."; exit 1; fi
	$(PYTHON) scripts/fetch_data.py $(TICKER) --start $(START) --end $(END)

fetch-all:
	@if [ ! -f "$(PYTHON)" ]; then echo "ERROR: .venv not found. Run 'make setup' first."; exit 1; fi
	$(PYTHON) scripts/fetch_data.py AAPL MSFT GOOG SPY NVDA --start $(START) --end $(END)

refresh:
	@if [ ! -f "$(PYTHON)" ]; then echo "ERROR: .venv not found. Run 'make setup' first."; exit 1; fi
	$(PYTHON) scripts/fetch_data.py $(SYMBOLS) --incremental
