"""Fetch real OHLCV data from Yahoo Finance and write to data/datasets/.

Usage:
    python scripts/fetch_data.py AAPL MSFT SPY
    python scripts/fetch_data.py AAPL --start 2020-01-01 --end 2024-12-31
    make fetch TICKER=NVDA
    make fetch-all
"""

import argparse
import sys
from pathlib import Path

import pandas as pd
import yfinance as yf


def transform_df(df: pd.DataFrame) -> pd.DataFrame:
    """Convert a yfinance download result to canonical OHLCV format.

    Input: DataFrame with DatetimeIndex (name='Date') and columns
    Open, High, Low, Close, Volume (as returned by yf.download with
    auto_adjust=True). Handles MultiIndex columns (multi-ticker downloads).

    Output: DataFrame with columns ['Date', 'Open', 'High', 'Low', 'Close',
    'Volume'], dates as YYYY-MM-DD strings, Volume as int, sorted ascending.
    """
    df = df.copy()

    # MultiIndex columns appear when downloading multiple tickers at once
    if isinstance(df.columns, pd.MultiIndex):
        df.columns = df.columns.get_level_values(0)

    df.index.name = "Date"
    df = df.reset_index()

    df = df[["Date", "Open", "High", "Low", "Close", "Volume"]]
    df["Date"] = pd.to_datetime(df["Date"]).dt.strftime("%Y-%m-%d")
    df["Volume"] = df["Volume"].fillna(0).astype(int)
    df = df.sort_values("Date").reset_index(drop=True)

    return df


def fetch_ticker(ticker: str, start: str, end: str, out_dir: Path) -> bool:
    """Fetch one ticker and write to out_dir/{TICKER}.csv. Returns True on success."""
    print(f"Fetching {ticker}...", end=" ", flush=True)

    try:
        raw = yf.download(ticker, start=start, end=end, auto_adjust=True, progress=False)
    except Exception as e:
        print(f"Error fetching {ticker}: {e} — skipping")
        return False

    if raw.empty:
        print(f"Warning: no data returned for {ticker} — skipping")
        return False

    try:
        df = transform_df(raw)
    except Exception as e:
        print(f"Error transforming {ticker}: {e} — skipping")
        return False

    out_path = out_dir / f"{ticker.upper()}.csv"
    df.to_csv(out_path, index=False)
    print(f"{len(df)} rows → {out_path}")
    return True


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Fetch real adjusted OHLCV and write to data/datasets/"
    )
    parser.add_argument("tickers", nargs="+", help="Ticker symbols (e.g. AAPL MSFT SPY)")
    parser.add_argument("--start", default="2020-01-01", help="Start date YYYY-MM-DD")
    parser.add_argument("--end", default="2024-12-31", help="End date YYYY-MM-DD")
    parser.add_argument("--out", default="data/datasets", help="Output directory")
    args = parser.parse_args()

    out_dir = Path(args.out)
    if not out_dir.exists():
        print(f"Error: output directory '{out_dir}' does not exist", file=sys.stderr)
        sys.exit(1)

    ok, failed = 0, []
    for ticker in args.tickers:
        t = ticker.upper()
        success = fetch_ticker(t, args.start, args.end, out_dir)
        if success:
            ok += 1
        else:
            failed.append(t)

    suffix = f" ({', '.join(failed)})" if failed else ""
    print(f"\nDone: {ok} fetched, {len(failed)} skipped{suffix}")
    if failed:
        sys.exit(1)


if __name__ == "__main__":
    main()
