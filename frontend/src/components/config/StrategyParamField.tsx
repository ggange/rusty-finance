import { Field } from "../ui/Field";
import type { StrategyParamMeta } from "../../types/api";

interface StrategyParamFieldProps {
  meta: StrategyParamMeta;
  value: number;
  onChange: (value: number) => void;
}

function prettify(name: string): string {
  return name
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

export function StrategyParamField({
  meta,
  value,
  onChange,
}: StrategyParamFieldProps) {
  return (
    <Field label={prettify(meta.name)} hint={meta.description} htmlFor={meta.name}>
      <input
        id={meta.name}
        type="number"
        min={meta.min}
        step={meta.step ?? (meta.type === "number" ? 0.5 : 1)}
        value={Number.isFinite(value) ? value : ""}
        onChange={(e) => {
          const v = Number(e.target.value);
          onChange(meta.type === "number" ? v : Math.trunc(v));
        }}
        className="w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none"
      />
    </Field>
  );
}
