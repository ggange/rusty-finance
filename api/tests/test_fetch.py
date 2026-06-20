"""Unit tests for scripts/fetch_data.py — no network calls."""

import sys
from pathlib import Path

import pandas as pd
import pytest

# scripts/ lives two levels above api/tests/
sys.path.insert(0, str(Path(__file__).parents[2] / "scripts"))

from fetch_data import fetch_ticker, transform_df


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
