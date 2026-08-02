"""Unit tests for scripts/fetch_data.py — no network calls."""

import sys
from pathlib import Path

import pandas as pd
import pytest

# scripts/ lives two levels above api/tests/
sys.path.insert(0, str(Path(__file__).parents[2] / "scripts"))

from fetch_data import (
    fetch_incremental,
    fetch_ticker,
    last_date_in_csv,
    merge_candles,
    transform_df,
)


def _mock_df() -> pd.DataFrame:
    """Minimal yfinance-shaped DataFrame (single ticker, auto_adjust=True)."""
    dates = pd.to_datetime(["2024-01-02", "2024-01-03", "2024-01-04"])
    return pd.DataFrame(
        {
            "Open": [150.0, 151.0, 152.0],
            "High": [151.5, 152.5, 153.5],
            "Low": [149.5, 150.5, 151.5],
            "Close": [151.0, 152.0, 153.0],
            "Volume": [10_000_000.0, 11_000_000.0, 12_000_000.0],
        },
        index=pd.DatetimeIndex(dates, name="Date"),
    )


def test_transform_df_produces_canonical_columns():
    df = transform_df(_mock_df())
    assert list(df.columns) == ["Date", "Open", "High", "Low", "Close", "Volume"]


def test_transform_df_formats_dates_as_strings():
    df = transform_df(_mock_df())
    assert list(df["Date"]) == ["2024-01-02", "2024-01-03", "2024-01-04"]


def test_transform_df_casts_volume_to_int():
    df = transform_df(_mock_df())
    assert df["Volume"].dtype == int
    assert df["Volume"].iloc[0] == 10_000_000


def test_transform_df_sorts_ascending():
    raw = _mock_df().iloc[::-1]  # reverse to descending
    df = transform_df(raw)
    assert df["Date"].is_monotonic_increasing


def test_transform_df_handles_multiindex_columns():
    raw = _mock_df()
    raw.columns = pd.MultiIndex.from_tuples(
        [(c, "AAPL") for c in raw.columns], names=["Price", "Ticker"]
    )
    df = transform_df(raw)
    assert list(df.columns) == ["Date", "Open", "High", "Low", "Close", "Volume"]


def test_fetch_ticker_skips_empty_df(tmp_path, monkeypatch):
    import yfinance as yf

    monkeypatch.setattr(yf, "download", lambda *a, **kw: pd.DataFrame())
    result = fetch_ticker("BOGUS", "2024-01-01", "2024-12-31", tmp_path)

    assert result is False
    assert not (tmp_path / "BOGUS.csv").exists()


def test_fetch_ticker_writes_csv(tmp_path, monkeypatch):
    import yfinance as yf

    monkeypatch.setattr(yf, "download", lambda *a, **kw: _mock_df())
    result = fetch_ticker("AAPL", "2024-01-01", "2024-12-31", tmp_path)

    assert result is True
    out = tmp_path / "AAPL.csv"
    assert out.exists()
    written = pd.read_csv(out)
    assert list(written.columns) == ["Date", "Open", "High", "Low", "Close", "Volume"]
    assert len(written) == 3


# ─── Incremental refresh ──────────────────────────────────────────────────────

def _write(path: Path, rows: list[tuple]) -> None:
    """Write a canonical OHLCV CSV from (date, close) tuples."""
    pd.DataFrame(
        [
            {"Date": d, "Open": c, "High": c, "Low": c, "Close": c, "Volume": 1000}
            for d, c in rows
        ]
    ).to_csv(path, index=False)


def _yf_df(rows: list[tuple]) -> pd.DataFrame:
    """Build a yfinance-shaped DataFrame from (date, close) tuples."""
    return pd.DataFrame(
        {
            "Open": [c for _, c in rows],
            "High": [c for _, c in rows],
            "Low": [c for _, c in rows],
            "Close": [c for _, c in rows],
            "Volume": [1000.0] * len(rows),
        },
        index=pd.DatetimeIndex(pd.to_datetime([d for d, _ in rows]), name="Date"),
    )


def test_last_date_in_csv_reads_final_row(tmp_path):
    p = tmp_path / "AAPL.csv"
    _write(p, [("2024-01-02", 100.0), ("2024-01-03", 101.0)])
    assert last_date_in_csv(p) == "2024-01-03"


def test_last_date_in_csv_missing_file_returns_none(tmp_path):
    assert last_date_in_csv(tmp_path / "NOPE.csv") is None


def test_last_date_in_csv_empty_file_returns_none(tmp_path):
    p = tmp_path / "EMPTY.csv"
    p.write_text("Date,Open,High,Low,Close,Volume\n")
    assert last_date_in_csv(p) is None


def test_merge_candles_appends_new_bars():
    old = transform_df(_yf_df([("2024-01-02", 100.0), ("2024-01-03", 101.0)]))
    new = transform_df(_yf_df([("2024-01-04", 102.0)]))
    merged = merge_candles(old, new)
    assert list(merged["Date"]) == ["2024-01-02", "2024-01-03", "2024-01-04"]


def test_merge_candles_dedupes_with_revision_winning():
    old = transform_df(_yf_df([("2024-01-02", 100.0), ("2024-01-03", 101.0)]))
    new = transform_df(_yf_df([("2024-01-03", 999.0), ("2024-01-04", 102.0)]))
    merged = merge_candles(old, new)

    assert list(merged["Date"]) == ["2024-01-02", "2024-01-03", "2024-01-04"]
    # the re-fetched 01-03 bar should carry the corrected close
    assert merged.loc[merged["Date"] == "2024-01-03", "Close"].iloc[0] == 999.0


def test_merge_candles_stays_sorted_when_new_is_out_of_order():
    old = transform_df(_yf_df([("2024-01-04", 102.0)]))
    new = transform_df(_yf_df([("2024-01-02", 100.0)]))
    merged = merge_candles(old, new)
    assert merged["Date"].is_monotonic_increasing


def test_fetch_incremental_appends_only_new_rows(tmp_path, monkeypatch):
    import yfinance as yf

    p = tmp_path / "AAPL.csv"
    _write(p, [("2024-01-02", 100.0), ("2024-01-03", 101.0)])

    monkeypatch.setattr(
        yf, "download",
        lambda *a, **kw: _yf_df([("2024-01-03", 101.0), ("2024-01-04", 102.0)]),
    )
    result = fetch_incremental("AAPL", tmp_path)

    assert result["status"] == "ok"
    assert result["added"] == 1
    assert result["total"] == 3
    assert result["end"] == "2024-01-04"
    assert list(pd.read_csv(p)["Date"]) == ["2024-01-02", "2024-01-03", "2024-01-04"]


def test_fetch_incremental_is_noop_when_no_new_bars(tmp_path, monkeypatch):
    import yfinance as yf

    p = tmp_path / "AAPL.csv"
    _write(p, [("2024-01-02", 100.0), ("2024-01-03", 101.0)])

    monkeypatch.setattr(yf, "download", lambda *a, **kw: _yf_df([("2024-01-03", 101.0)]))
    result = fetch_incremental("AAPL", tmp_path)

    assert result["added"] == 0
    assert result["total"] == 2


def test_fetch_incremental_backfills_when_csv_absent(tmp_path, monkeypatch):
    import yfinance as yf

    monkeypatch.setattr(
        yf, "download",
        lambda *a, **kw: _yf_df([("2024-01-02", 100.0), ("2024-01-03", 101.0)]),
    )
    result = fetch_incremental("NEW", tmp_path)

    assert result["status"] == "ok"
    assert result["added"] == 2
    assert (tmp_path / "NEW.csv").exists()


def test_fetch_incremental_requests_from_last_date(tmp_path, monkeypatch):
    """The refetch window must start at the last stored bar so revisions are caught."""
    import yfinance as yf

    p = tmp_path / "AAPL.csv"
    _write(p, [("2024-01-02", 100.0), ("2024-01-03", 101.0)])

    seen = {}

    def spy(*a, **kw):
        seen.update(kw)
        return _yf_df([("2024-01-03", 101.0)])

    monkeypatch.setattr(yf, "download", spy)
    fetch_incremental("AAPL", tmp_path)

    assert seen["start"] == "2024-01-03"


def test_fetch_incremental_handles_empty_response(tmp_path, monkeypatch):
    import yfinance as yf

    p = tmp_path / "AAPL.csv"
    _write(p, [("2024-01-02", 100.0)])

    monkeypatch.setattr(yf, "download", lambda *a, **kw: pd.DataFrame())
    result = fetch_incremental("AAPL", tmp_path)

    assert result["status"] == "no_data"
    assert result["added"] == 0
    # existing data must survive an empty response
    assert list(pd.read_csv(p)["Date"]) == ["2024-01-02"]


def test_fetch_incremental_survives_download_error(tmp_path, monkeypatch):
    import yfinance as yf

    p = tmp_path / "AAPL.csv"
    _write(p, [("2024-01-02", 100.0)])

    def boom(*a, **kw):
        raise RuntimeError("network down")

    monkeypatch.setattr(yf, "download", boom)
    result = fetch_incremental("AAPL", tmp_path)

    assert result["status"] == "error"
    assert list(pd.read_csv(p)["Date"]) == ["2024-01-02"]
