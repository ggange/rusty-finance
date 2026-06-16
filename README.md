# rusty-finance

A financial backtesting monorepo: a high-performance Rust core, Python bindings via PyO3, and a FastAPI REST layer.

```
backtesting/          Rust library — strategies, engine, portfolio, metrics
backtesting-py/       PyO3 bridge — exposes run_ma() and run_rsi() to Python
api/                  FastAPI server — REST endpoints over the Python bindings
data/fixtures/        Synthetic CSV fixtures used by tests
```

## Architecture

```
backtesting (Rust core)
      ↓  path dependency
backtesting-py (PyO3 cdylib)
      ↓  import backtesting_py
api (FastAPI HTTP layer)
```

## Prerequisites

| Tool | Version |
|------|---------|
| Rust | stable (edition 2024) |
| Python | ≥ 3.9 |
| maturin | ≥ 1.0 |

Install maturin: `pip install maturin`

## Build & Test

### Rust core

```bash
cd backtesting
cargo test          # unit + integration tests
cargo doc --open    # browse rustdoc
```

### Python bindings

```bash
cd backtesting-py
maturin develop     # build and install into current virtualenv
```

### API

```bash
cd api
pip install -e ".[dev]"
uvicorn api.main:app --reload   # start server at http://localhost:8000
pytest tests/                   # run API tests
```

## CSV Format

The backtesting engine expects CSV files with these headers:

| Column | Type | Description |
|--------|------|-------------|
| Date | YYYY-MM-DD | Bar date |
| Open | float | Opening price |
| High | float | Period high |
| Low | float | Period low |
| Close | float | Closing price |
| Volume | integer | Shares traded |
