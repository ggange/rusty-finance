"""Fetch real OHLCV data from Yahoo Finance and write to data/datasets/.

Usage:
    python scripts/fetch_data.py AAPL MSFT SPY
    python scripts/fetch_data.py AAPL --start 2020-01-01 --end 2024-12-31
    python scripts/fetch_data.py --incremental AAPL MSFT   # append new bars only
    make fetch TICKER=NVDA
    make fetch-all
    make refresh
"""

import argparse
import sys
from datetime import date, timedelta
from pathlib import Path

import pandas as pd
import yfinance as yf

DEFAULT_START = "2020-01-01"


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


def last_date_in_csv(path: Path) -> str | None:
    """Return the last (max) date in an existing dataset CSV, or None if unusable.

    None means "no usable history" — missing file, empty file, or unparseable —
    and callers should treat it as a signal to backfill from DEFAULT_START.
    """
    if not path.is_file():
        return None
    try:
        df = pd.read_csv(path)
    except Exception:
        return None
    if df.empty or "Date" not in df.columns:
        return None
    dates = df["Date"].dropna().astype(str)
    if dates.empty:
        return None
    return max(dates)


def merge_candles(old: pd.DataFrame, new: pd.DataFrame) -> pd.DataFrame:
    """Merge new bars into existing history.

    Rows are deduplicated on Date with the *new* bar winning, so a re-fetched
    trailing bar picks up Yahoo's post-close revisions rather than keeping the
    stale copy. Result is sorted ascending with a clean index.
    """
    combined = pd.concat([old, new], ignore_index=True)
    combined = combined.drop_duplicates(subset="Date", keep="last")
    return combined.sort_values("Date").reset_index(drop=True)


def fetch_incremental(ticker: str, out_dir: Path, end: str | None = None) -> dict:
    """Append any bars newer than what's already on disk for one ticker.

    Refetches from the last stored bar (inclusive) through `end` (default:
    tomorrow, since yfinance treats end as exclusive), then merges. Existing
    data is never destroyed: on an empty response or a download error the CSV
    is left exactly as it was.

    Returns a dict: {ticker, status, added, total, start, end} where status is
    "ok" | "no_data" | "error".
    """
    t = ticker.upper()
    out_path = out_dir / f"{t}.csv"

    last = last_date_in_csv(out_path)
    start = last if last is not None else DEFAULT_START
    end = end or (date.today() + timedelta(days=1)).isoformat()

    existing = pd.DataFrame()
    if last is not None:
        try:
            existing = pd.read_csv(out_path)
            existing["Date"] = existing["Date"].astype(str)
        except Exception:
            existing = pd.DataFrame()

    before = len(existing)

    def _result(status: str, df: pd.DataFrame) -> dict:
        return {
            "ticker": t,
            "status": status,
            "added": max(0, len(df) - before),
            "total": len(df),
            "start": str(df["Date"].iloc[0]) if len(df) else None,
            "end": str(df["Date"].iloc[-1]) if len(df) else None,
        }

    try:
        raw = yf.download(t, start=start, end=end, auto_adjust=True, progress=False)
    except Exception as e:
        print(f"  {t}: error fetching: {e} — keeping existing data")
        return _result("error", existing)

    if raw.empty:
        print(f"  {t}: no new bars")
        return _result("no_data", existing)

    try:
        fresh = transform_df(raw)
    except Exception as e:
        print(f"  {t}: error transforming: {e} — keeping existing data")
        return _result("error", existing)

    merged = merge_candles(existing, fresh) if before else fresh
    merged.to_csv(out_path, index=False)

    res = _result("ok", merged)
    print(f"  {t}: +{res['added']} bars → {res['total']} total (through {res['end']})")
    return res


def refresh_all(symbols: list[str], out_dir: Path) -> list[dict]:
    """Incrementally refresh a list of symbols. Returns one result dict per symbol."""
    print(f"Refreshing {len(symbols)} symbol(s) in {out_dir}...")
    return [fetch_incremental(s, out_dir) for s in symbols]


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Fetch real adjusted OHLCV and write to data/datasets/"
    )
    parser.add_argument("tickers", nargs="+", help="Ticker symbols (e.g. AAPL MSFT SPY)")
    parser.add_argument("--start", default=DEFAULT_START, help="Start date YYYY-MM-DD")
    parser.add_argument("--end", default="2024-12-31", help="End date YYYY-MM-DD")
    parser.add_argument("--out", default="data/datasets", help="Output directory")
    parser.add_argument(
        "--incremental",
        action="store_true",
        help="Append only bars newer than what's on disk (ignores --start/--end)",
    )
    args = parser.parse_args()

    out_dir = Path(args.out)
    if not out_dir.exists():
        print(f"Error: output directory '{out_dir}' does not exist", file=sys.stderr)
        sys.exit(1)

    ok, failed = 0, []

    if args.incremental:
        for res in refresh_all([t.upper() for t in args.tickers], out_dir):
            if res["status"] == "error":
                failed.append(res["ticker"])
            else:
                ok += 1
        suffix = f" ({', '.join(failed)})" if failed else ""
        print(f"\nDone: {ok} refreshed, {len(failed)} failed{suffix}")
        if failed:
            sys.exit(1)
        return

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
