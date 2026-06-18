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

## Seed data

`AAPL.csv`, `MSFT.csv`, and `GOOG.csv` are synthetic 30-bar series with
deliberately different shapes (trend, oscillation, rally-then-fade) so a
multi-asset portfolio shows meaningful diversification. Replace them with real
data whenever you like — just drop in more CSVs.
