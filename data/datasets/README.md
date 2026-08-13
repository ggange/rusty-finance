# Dataset catalog

CSV files in this directory are the server-side data catalog. Each file is one
symbol's OHLCV history and shows up in the API's `GET /datasets` response and in
the frontend's per-asset **data source** picker.

## Conventions

- **One CSV per symbol.** The file name is the dataset id (e.g. `AAPL.csv`); the
  stem (`AAPL`) is used as the default symbol.
- **Headers:** `Date,Open,High,Low,Close,Volume` (case-insensitive; lowercase
  also accepted). `Volume` is optional.
- **Dates:** `YYYY-MM-DD`, chronological order.

## Location override

By default the API reads `<repo>/data/datasets`. Point it elsewhere with:

```bash
export RUSTY_FINANCE_DATA_DIR=/path/to/my/csvs
```

## Included datasets

`AAPL.csv`, `MSFT.csv`, `GOOG.csv`, `SPY.csv`, and `NVDA.csv` are real adjusted
OHLCV data fetched from Yahoo Finance (2020-01-01 → 2024-12-31, ~1260 bars each).
Prices are dividend/split-adjusted so returns are honest.

**Provenance and terms.** These bars were retrieved from Yahoo Finance via
[`yfinance`](https://github.com/ranaroussi/yfinance) and are bundled only as a
small fixed sample, so the repo is clone-and-run and published results are
reproducible. They are not offered as a data product, come with no warranty of
accuracy or completeness, and remain subject to Yahoo Finance's terms of use.
Regenerate them with `make fetch-all`, or supply your own licensed data and
point the API at it with `RUSTY_FINANCE_DATA_DIR`.

## Fetching more data

Use the included fetcher script to add any ticker:

```bash
# One ticker
make fetch TICKER=TSLA

# Custom date range
make fetch TICKER=AMZN START=2015-01-01 END=2024-12-31

# Refresh all bundled datasets
make fetch-all
```

Or directly:

```bash
python scripts/fetch_data.py TSLA AMZN BRK-B --start 2018-01-01 --end 2024-12-31
```

New CSVs appear in the dataset picker immediately without restarting the API.
