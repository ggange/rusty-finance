import { Field } from "../ui/Field";
import { Input } from "../ui/Input";
import type { StrategyParamMeta } from "../../types/api";

interface StrategyParamFieldProps {
  meta: StrategyParamMeta;
  value: number;
  onChange: (value: number) => void;
  /** Prefix so ids stay unique when several assets render the same param. */
  idPrefix?: string;
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
  idPrefix = "",
}: StrategyParamFieldProps) {
  const id = `${idPrefix}${meta.name}`;
  return (
    <Field label={prettify(meta.name)} hint={meta.description} htmlFor={id}>
      <Input
        id={id}
        type="number"
        min={meta.min}
        step={meta.step ?? (meta.type === "number" ? 0.5 : 1)}
        value={Number.isFinite(value) ? value : ""}
        onChange={(e) => {
          const v = Number(e.target.value);
          onChange(meta.type === "number" ? v : Math.trunc(v));
        }}
      />
    </Field>
  );
}
