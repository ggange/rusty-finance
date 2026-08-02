import { Field } from "../ui/Field";
import { Select } from "../ui/Select";
import { StrategyParamField } from "./StrategyParamField";
import type { StrategyMeta, StrategyType } from "../../types/api";

interface StrategyPickerProps {
  strategies: StrategyMeta[];
  strategyType: StrategyType | null;
  current: StrategyMeta | null;
  params: Record<string, number>;
  onTypeChange: (type: StrategyType) => void;
  onParam: (name: string, value: number) => void;
  /** Prefix so ids stay unique when several assets each render a picker. */
  idPrefix?: string;
}

export function StrategyPicker({
  strategies,
  strategyType,
  current,
  params,
  onTypeChange,
  onParam,
  idPrefix = "",
}: StrategyPickerProps) {
  const selectId = `${idPrefix}strategy-select`;
  return (
    <div className="space-y-3">
      <Field label="Strategy" htmlFor={selectId}>
        <Select
          id={selectId}
          value={strategyType ?? ""}
          onChange={(e) => onTypeChange(e.target.value as StrategyType)}
        >
          {strategies.map((s) => (
            <option key={s.type} value={s.type}>
              {s.name}
            </option>
          ))}
        </Select>
      </Field>

      {current && (
        <p className="text-xs text-slate-500">{current.description}</p>
      )}

      {current?.params.map((p) => (
        <StrategyParamField
          key={p.name}
          meta={p}
          idPrefix={idPrefix}
          value={params[p.name] ?? p.default}
          onChange={(v) => onParam(p.name, v)}
        />
      ))}
    </div>
  );
}
