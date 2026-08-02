import { useRef, useState } from "react";
import { api } from "../../lib/apiClient";
import { parseCsv } from "../../lib/csv";
import type { Candle, Dataset } from "../../types/api";
import type { AssetSourceState } from "../../state/usePortfolioForm";
import { Button } from "../ui/Button";
import { Select } from "../ui/Select";

interface AssetDataSourceProps {
  datasets: Dataset[];
  source: AssetSourceState;
  candles: Candle[];
  onDataset: (name: string, candles: Candle[], symbol: string) => void;
  onInline: (fileName: string, candles: Candle[]) => void;
}

export function AssetDataSource({
  datasets,
  source,
  candles,
  onDataset,
  onInline,
}: AssetDataSourceProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function pickDataset(name: string) {
    if (!name) return;
    setError(null);
    setLoading(true);
    try {
      const { candles: rows } = await api.dataset(name);
      onDataset(name, rows, name.replace(/\.csv$/i, ""));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function loadFile(file: File) {
    setError(null);
    try {
      const parsed = await parseCsv(file);
      onInline(file.name, parsed);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const selectedName = source.kind === "dataset" ? source.name : "";
  const summary =
    candles.length > 0
      ? `${candles.length} candles · ${candles[0].date} → ${candles[candles.length - 1].date}`
      : null;

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <Select
          size="sm"
          value={selectedName}
          onChange={(e) => void pickDataset(e.target.value)}
          aria-label="Dataset"
        >
          <option value="">
            {datasets.length ? "Pick a dataset…" : "No datasets on server"}
          </option>
          {datasets.map((d) => (
            <option key={d.name} value={d.name}>
              {d.symbol} ({d.rows} bars)
            </option>
          ))}
        </Select>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => inputRef.current?.click()}
          className="shrink-0 py-1.5"
        >
          Upload…
        </Button>
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

      {loading && <p className="text-xs text-slate-400">Loading dataset…</p>}
      {summary && (
        <p className="text-xs text-emerald-300">
          {source.kind === "inline" ? "uploaded · " : ""}
          {summary}
        </p>
      )}
      {error && <p className="text-xs text-rose-300">{error}</p>}
    </div>
  );
}
