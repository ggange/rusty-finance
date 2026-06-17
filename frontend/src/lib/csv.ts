import Papa from "papaparse";
import type { Candle } from "../types/api";

// A parsed CSV row. Headers may be uppercase (Date, Open, ...) as in the Rust
// fixtures, or lowercase. dynamicTyping coerces numeric columns to numbers.
type RawRow = Record<string, string | number | null>;

function pick(row: RawRow, ...keys: string[]): string | number | null {
  for (const k of keys) {
    if (row[k] !== undefined && row[k] !== null && row[k] !== "") return row[k];
  }
  return null;
}

function toNumber(value: string | number | null, field: string, line: number): number {
  const n = typeof value === "number" ? value : Number(value);
  if (value === null || Number.isNaN(n) || !Number.isFinite(n)) {
    throw new Error(`Row ${line}: invalid ${field} value "${value}"`);
  }
  return n;
}

function toDate(value: string | number | null, line: number): string {
  if (value === null) throw new Error(`Row ${line}: missing date`);
  const s = String(value).trim();
  // Accept YYYY-MM-DD (the format the API and charts expect).
  if (!/^\d{4}-\d{2}-\d{2}$/.test(s)) {
    throw new Error(`Row ${line}: date "${s}" is not YYYY-MM-DD`);
  }
  return s;
}

function rowToCandle(row: RawRow, index: number): Candle {
  const line = index + 2; // +1 for header, +1 for 1-based display
  return {
    date: toDate(pick(row, "Date", "date"), line),
    open: toNumber(pick(row, "Open", "open"), "open", line),
    high: toNumber(pick(row, "High", "high"), "high", line),
    low: toNumber(pick(row, "Low", "low"), "low", line),
    close: toNumber(pick(row, "Close", "close"), "close", line),
    volume: (() => {
      const v = pick(row, "Volume", "volume");
      return v === null ? 0 : toNumber(v, "volume", line);
    })(),
  };
}

/**
 * Parse a CSV File (upload) or raw CSV string (bundled sample) into candles.
 * Maps uppercase or lowercase headers to the lowercase keys the API expects,
 * validates each row, and returns candles sorted ascending by date.
 * Throws a friendly Error on malformed input.
 */
export function parseCsv(input: File | string): Promise<Candle[]> {
  return new Promise((resolve, reject) => {
    Papa.parse<RawRow>(input as never, {
      header: true,
      dynamicTyping: true,
      skipEmptyLines: true,
      complete: (results) => {
        try {
          if (results.errors.length > 0) {
            const e = results.errors[0];
            throw new Error(`CSV parse error: ${e.message} (row ${e.row ?? "?"})`);
          }
          const rows = results.data.filter(
            (r) => r && Object.keys(r).length > 0,
          );
          if (rows.length === 0) throw new Error("CSV contains no data rows");

          const candles = rows.map(rowToCandle);
          candles.sort((a, b) => a.date.localeCompare(b.date));
          resolve(candles);
        } catch (err) {
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      },
      error: (err: Error) => reject(err),
    });
  });
}
