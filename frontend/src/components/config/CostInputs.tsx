import { Field } from "../ui/Field";

interface CostInputsProps {
  initialCash: number;
  commission: number;
  slippagePct: number; // fraction (0–1)
  onInitialCash: (v: number) => void;
  onCommission: (v: number) => void;
  onSlippagePct: (v: number) => void;
}

const numberClass =
  "w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none";

export function CostInputs({
  initialCash,
  commission,
  slippagePct,
  onInitialCash,
  onCommission,
  onSlippagePct,
}: CostInputsProps) {
  return (
    <div className="space-y-3">
      <Field label="Initial cash ($)" htmlFor="initial-cash">
        <input
          id="initial-cash"
          type="number"
          min={1}
          step={100}
          value={Number.isFinite(initialCash) ? initialCash : ""}
          onChange={(e) => onInitialCash(Number(e.target.value))}
          className={numberClass}
        />
      </Field>

      <Field label="Commission per trade ($)" htmlFor="commission">
        <input
          id="commission"
          type="number"
          min={0}
          step={1}
          value={Number.isFinite(commission) ? commission : ""}
          onChange={(e) => onCommission(Number(e.target.value))}
          className={numberClass}
        />
      </Field>

      <Field
        label="Slippage (%)"
        hint="One-way price penalty, 0–99.9%"
        htmlFor="slippage"
      >
        <input
          id="slippage"
          type="number"
          min={0}
          max={99.9}
          step={0.1}
          value={Number.isFinite(slippagePct) ? slippagePct * 100 : ""}
          onChange={(e) => onSlippagePct(Number(e.target.value) / 100)}
          className={numberClass}
        />
      </Field>
    </div>
  );
}
