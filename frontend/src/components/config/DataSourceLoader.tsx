import { useRef, useState } from "react";
import { parseCsv } from "../../lib/csv";
import type { Candle } from "../../types/api";

interface DataSourceLoaderProps {
  candles: Candle[];
  fileName: string | null;
  onCandles: (candles: Candle[], fileName: string) => void;
}

const SAMPLE_URL = "/sample/synthetic_30.csv";

export function DataSourceLoader({
  candles,
  fileName,
  onCandles,
}: DataSourceLoaderProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);

  async function loadSample() {
    setError(null);
    try {
      const text = await fetch(SAMPLE_URL).then((r) => {
        if (!r.ok) throw new Error("Could not load the bundled sample.");
        return r.text();
      });
      const parsed = await parseCsv(text);
      onCandles(parsed, "synthetic_30.csv");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function loadFile(file: File) {
    setError(null);
    try {
      const parsed = await parseCsv(file);
      onCandles(parsed, file.name);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const summary =
    candles.length > 0
      ? `${candles.length} candles · ${fileName ?? "uploaded"} · ${candles[0].date} → ${candles[candles.length - 1].date}`
      : null;

  return (
    <div className="space-y-3">
      <div
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          const file = e.dataTransfer.files[0];
          if (file) void loadFile(file);
        }}
        className={`flex flex-col items-center justify-center rounded-lg border-2 border-dashed p-4 text-center text-sm transition ${
          dragging
            ? "border-sky-400 bg-sky-400/10"
            : "border-slate-600 bg-slate-900/40"
        }`}
      >
        <p className="text-slate-400">Drag &amp; drop a CSV here, or</p>
        <button
          type="button"
          onClick={() => inputRef.current?.click()}
          className="mt-2 rounded-md bg-slate-700 px-3 py-1.5 text-sm font-medium text-slate-100 hover:bg-slate-600"
        >
          Choose file…
        </button>
        <input
          ref={inputRef}
          type="file"
          accept=".csv,text/csv"
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) void loadFile(file);
            e.target.value = "";
          }}
        />
      </div>

      <button
        type="button"
        onClick={() => void loadSample()}
        className="w-full rounded-md border border-slate-600 px-3 py-1.5 text-sm font-medium text-sky-300 hover:bg-slate-700/60"
      >
        Load sample (synthetic_30.csv)
      </button>

      {summary && (
        <p className="rounded-md bg-slate-900/60 px-3 py-2 text-xs text-emerald-300">
          {summary}
        </p>
      )}
      {error && (
        <p className="rounded-md bg-rose-950/60 px-3 py-2 text-xs text-rose-300">
          {error}
        </p>
      )}
    </div>
  );
}
