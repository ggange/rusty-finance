import { Field } from "../ui/Field";
import { Input } from "../ui/Input";
import type { ParamRange, StrategyParamMeta } from "../../types/api";

interface ParamRangeGridProps {
  params: StrategyParamMeta[];
  ranges: Record<string, ParamRange>;
  onChange: (name: string, field: keyof ParamRange, value: number) => void;
  /** Prefix for input ids, so sweep and walk-forward forms don't collide. */
  idPrefix: string;
}

/**
 * Min/max/step grid for a strategy's parameters. Shared by the sweep and
 * walk-forward forms, which build identical grids over the same metadata.
 */
export function ParamRangeGrid({
  params,
  ranges,
  onChange,
  idPrefix,
}: ParamRangeGridProps) {
  return (
    <div className="space-y-4">
      {params.map((p) => {
        const rng = ranges[p.name] ?? { min: p.default, max: p.default, step: 1 };
        const fields: Array<{ key: keyof ParamRange; label: string; min: number }> = [
          { key: "min", label: "Min", min: p.min },
          { key: "max", label: "Max", min: p.min },
          { key: "step", label: "Step", min: 0.001 },
        ];
        return (
          <div key={p.name}>
            <p className="mb-1.5 text-sm font-medium text-slate-300">{p.name}</p>
            <div className="grid grid-cols-3 gap-2">
              {fields.map((f) => {
                const id = `${idPrefix}${p.name}-${f.key}`;
                return (
                  <Field key={f.key} label={f.label} htmlFor={id}>
                    <Input
                      id={id}
                      type="number"
                      min={f.min}
                      step={p.step ?? 1}
                      value={rng[f.key]}
                      onChange={(e) => onChange(p.name, f.key, Number(e.target.value))}
                    />
                  </Field>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}
