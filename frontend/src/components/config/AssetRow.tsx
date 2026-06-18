import { AssetDataSource } from "./AssetDataSource";
import { StrategyPicker } from "./StrategyPicker";
import { Field } from "../ui/Field";
import type { Dataset, StrategyMeta, StrategyType } from "../../types/api";
import type { AssetConfig, PortfolioForm } from "../../state/usePortfolioForm";

interface AssetRowProps {
  index: number;
  asset: AssetConfig;
  strategies: StrategyMeta[];
  datasets: Dataset[];
  form: PortfolioForm;
  canRemove: boolean;
  weightPct: number | null;
}

const inputClass =
  "w-full rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none";

export function AssetRow({
  index,
  asset,
  strategies,
  datasets,
  form,
  canRemove,
  weightPct,
}: AssetRowProps) {
  const current = strategies.find((s) => s.type === asset.strategyType) ?? null;

  return (
    <div className="space-y-3 rounded-lg border border-slate-700 bg-slate-900/40 p-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-wide text-slate-400">
          Asset {index + 1}
        </span>
        {canRemove && (
          <button
            type="button"
            onClick={() => form.removeAsset(asset.id)}
            className="text-xs text-slate-500 hover:text-rose-400"
            aria-label={`Remove asset ${index + 1}`}
          >
            Remove
          </button>
        )}
      </div>

      <div className="grid grid-cols-[1fr_auto] gap-2">
        <Field label="Symbol" htmlFor={`${asset.id}-symbol`}>
          <input
            id={`${asset.id}-symbol`}
            type="text"
            value={asset.symbol}
            placeholder="e.g. AAPL"
            onChange={(e) => form.updateAsset(asset.id, { symbol: e.target.value })}
            className={inputClass}
          />
        </Field>
        <Field
          label="Weight"
          hint={weightPct !== null ? `${weightPct.toFixed(0)}%` : undefined}
          htmlFor={`${asset.id}-weight`}
        >
          <input
            id={`${asset.id}-weight`}
            type="number"
            min={0}
            step={0.1}
            value={Number.isFinite(asset.weight) ? asset.weight : ""}
            onChange={(e) =>
              form.updateAsset(asset.id, { weight: Number(e.target.value) })
            }
            className={`${inputClass} w-24`}
          />
        </Field>
      </div>

      <div>
        <p className="mb-1 text-xs text-slate-500">Data source</p>
        <AssetDataSource
          datasets={datasets}
          source={asset.source}
          candles={asset.candles}
          onDataset={(name, candles, symbol) =>
            form.setAssetSourceDataset(asset.id, name, candles, symbol)
          }
          onInline={(fileName, candles) =>
            form.setAssetSourceInline(asset.id, fileName, candles)
          }
        />
      </div>

      <StrategyPicker
        strategies={strategies}
        strategyType={asset.strategyType}
        current={current}
        params={asset.params}
        onTypeChange={(t: StrategyType) => form.setAssetStrategyType(asset.id, t)}
        onParam={(name, v) => form.setAssetParam(asset.id, name, v)}
      />
    </div>
  );
}
