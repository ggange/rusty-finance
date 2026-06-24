import csv
import json
import os
from pathlib import Path

from fastapi import HTTPException


def _data_dir() -> Path:
    """Directory holding one CSV per symbol. Override with RUSTY_FINANCE_DATA_DIR."""
    env = os.environ.get("RUSTY_FINANCE_DATA_DIR")
    if env:
        return Path(env)
    return Path(__file__).resolve().parent.parent / "data" / "datasets"


def _read_csv_candles(path: Path) -> list[dict]:
    """Parse a CSV into candle dicts, tolerating both upper- and lowercase headers."""
    def pick(row: dict, *keys: str):
        for k in keys:
            if row.get(k) not in (None, ""):
                return row[k]
        return None

    with path.open(newline="") as f:
        out = []
        for row in csv.DictReader(f):
            vol = pick(row, "Volume", "volume")
            out.append({
                "date": pick(row, "Date", "date"),
                "open": float(pick(row, "Open", "open")),
                "high": float(pick(row, "High", "high")),
                "low": float(pick(row, "Low", "low")),
                "close": float(pick(row, "Close", "close")),
                "volume": int(float(vol)) if vol is not None else 0,
            })
        return out


def list_datasets() -> list[dict]:
    """Enumerate available CSV datasets with light metadata. Missing dir → []."""
    data_dir = _data_dir()
    if not data_dir.is_dir():
        return []
    datasets = []
    for path in sorted(data_dir.glob("*.csv")):
        try:
            candles = _read_csv_candles(path)
        except Exception:
            continue
        if not candles:
            continue
        datasets.append({
            "name": path.name,
            "symbol": path.stem,
            "rows": len(candles),
            "start": candles[0]["date"],
            "end": candles[-1]["date"],
        })
    return datasets


def load_dataset(name: str) -> list[dict]:
    """Load a named dataset's candles, guarding against path traversal."""
    safe = os.path.basename(name)
    if safe != name or not safe:
        raise HTTPException(status_code=422, detail=f"invalid dataset name: {name!r}")
    path = _data_dir() / safe
    if not path.is_file():
        raise HTTPException(status_code=404, detail=f"dataset not found: {name!r}")
    return _read_csv_candles(path)


def candles_to_json(candles: list[dict]) -> str:
    return json.dumps(candles)
