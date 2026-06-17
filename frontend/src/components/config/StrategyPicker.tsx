import { Field } from "../ui/Field";
import { StrategyParamField } from "./StrategyParamField";
import type { StrategyMeta, StrategyType } from "../../types/api";

interface StrategyPickerProps {
  strategies: StrategyMeta[];
  strategyType: StrategyType | null;
  current: StrategyMeta | null;
  params: Record<string, number>;
  onTypeChange: (type: StrategyType) => void;
  onParam: (name: string, value: number) => void;
}

export function StrategyPicker({
  strategies,
  strategyType,
  current,
  params,
  onTypeChange,
  onParam,
}: StrategyPickerProps) {
  return (
    <div className="space-y-3">
      <Field label="Strategy" htmlFor="strategy-select">
        <select
          id="strategy-select"
          value={strategyType ?? ""}
          onChange={(e) => onTypeChange(e.target.value as StrategyType)}
          className="w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none"
        >
          {strategies.map((s) => (
            <option key={s.type} value={s.type}>
              {s.name}
            </option>
          ))}
        </select>
      </Field>

      {current && (
        <p className="text-xs text-slate-500">{current.description}</p>
      )}

      {current?.params.map((p) => (
        <StrategyParamField
          key={p.name}
          meta={p}
          value={params[p.name] ?? p.default}
          onChange={(v) => onParam(p.name, v)}
        />
      ))}
    </div>
  );
}
