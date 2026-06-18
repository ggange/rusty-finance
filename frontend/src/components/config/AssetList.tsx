import { AssetRow } from "./AssetRow";
import type { Dataset, StrategyMeta } from "../../types/api";
import type { PortfolioForm } from "../../state/usePortfolioForm";

interface AssetListProps {
  strategies: StrategyMeta[];
  datasets: Dataset[];
  form: PortfolioForm;
}

export function AssetList({ strategies, datasets, form }: AssetListProps) {
  const weightSum = form.assets.reduce((s, a) => s + Math.max(0, a.weight), 0);

  return (
    <div className="space-y-3">
      {form.assets.map((asset, i) => (
        <AssetRow
          key={asset.id}
          index={i}
          asset={asset}
          strategies={strategies}
          datasets={datasets}
          form={form}
          canRemove={form.assets.length > 1}
          weightPct={weightSum > 0 ? (Math.max(0, asset.weight) / weightSum) * 100 : null}
        />
      ))}

      <button
        type="button"
        onClick={form.addAsset}
        className="w-full rounded-md border border-dashed border-slate-600 px-3 py-2 text-sm font-medium text-sky-300 hover:bg-slate-700/40"
      >
        + Add asset
      </button>

      <p className="text-center text-xs text-slate-500">
        Capital is split by weight (normalized). Weights sum to{" "}
        {weightSum.toFixed(1)}.
      </p>
    </div>
  );
}
