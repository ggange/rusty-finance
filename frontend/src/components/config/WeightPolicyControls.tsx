import { useEffect, useState } from "react";
import { Field } from "../ui/Field";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { StatusPill } from "../ui/StatusPill";
import {
  RETURN_DEPENDENT_OBJECTIVES,
  type Objective,
  type OptimizerConfig,
  type WeightPolicy,
  type WeightPolicyKind,
} from "../../types/api";

const OBJECTIVES: Array<{ value: Objective; label: string; hint: string }> = [
  {
    value: "risk_parity",
    label: "Risk parity",
    hint: "Every asset contributes the same share of portfolio risk.",
  },
  {
    value: "min_variance",
    label: "Minimum variance",
    hint: "Lowest portfolio variance. Uses correlation, so it can overweight a volatile asset that diversifies.",
  },
  {
    value: "inverse_volatility",
    label: "Inverse volatility",
    hint: "Weight ∝ 1/σ. Ignores correlation entirely.",
  },
  {
    value: "equal_weight",
    label: "Equal weight",
    hint: "1/n. The baseline that is hard to beat out-of-sample.",
  },
  {
    value: "max_sharpe",
    label: "Maximum Sharpe",
    hint: "Highest in-sample Sharpe. Depends on mean returns, which estimate poorly.",
  },
];

export const DEFAULT_OPTIMIZER: OptimizerConfig = {
  objective: "risk_parity",
  shrinkage: 0.2,
  max_weight: null,
};

/** The policy a given kind starts from when the user switches to it. */
export function defaultPolicy(kind: WeightPolicyKind): WeightPolicy {
  switch (kind) {
    case "manual":
      return { kind: "manual" };
    case "static":
      return { kind: "static", optimizer: { ...DEFAULT_OPTIMIZER }, warmup: 252 };
    case "dynamic":
      return { kind: "dynamic", optimizer: { ...DEFAULT_OPTIMIZER }, lookback: 252 };
  }
}

/**
 * A whole-number field with a minimum, which still lets you clear and retype.
 *
 * Clamping on every keystroke makes the control unusable: clearing "252" snaps
 * it straight back to the minimum, so the next digit typed appends to that
 * instead of replacing it. The draft is held locally while editing and only
 * clamped once it parses, or on blur.
 */
function BarsInput({
  id,
  value,
  min,
  onCommit,
}: {
  id: string;
  value: number;
  min: number;
  onCommit: (v: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));

  // Re-sync when the value changes from outside (e.g. switching policy kind).
  useEffect(() => {
    setDraft(String(value));
  }, [value]);

  return (
    <Input
      id={id}
      type="number"
      min={min}
      step={1}
      value={draft}
      onChange={(e) => {
        const raw = e.target.value;
        setDraft(raw);
        const parsed = Number(raw);
        if (raw.trim() !== "" && Number.isFinite(parsed) && parsed >= min) {
          onCommit(Math.trunc(parsed));
        }
      }}
      onBlur={() => setDraft(String(value))}
    />
  );
}

interface WeightPolicyControlsProps {
  value: WeightPolicy;
  onChange: (policy: WeightPolicy) => void;
  /** Rebalancing is what gives a dynamic policy its re-solve dates. */
  hasRebalance: boolean;
}

export function WeightPolicyControls({
  value,
  onChange,
  hasRebalance,
}: WeightPolicyControlsProps) {
  const solving = value.kind !== "manual";
  const optimizer = solving ? value.optimizer : DEFAULT_OPTIMIZER;
  const meta = OBJECTIVES.find((o) => o.value === optimizer.objective);

  function setOptimizer(patch: Partial<OptimizerConfig>) {
    if (!solving) return;
    onChange({ ...value, optimizer: { ...value.optimizer, ...patch } });
  }

  return (
    <div className="space-y-3">
      <Field
        label="Weights"
        htmlFor="weight-policy"
        hint="Manual uses the weights above; the others solve them."
      >
        <Select
          id="weight-policy"
          value={value.kind}
          onChange={(e) => onChange(defaultPolicy(e.target.value as WeightPolicyKind))}
        >
          <option value="manual">Manual</option>
          <option value="static">Solve once (static)</option>
          <option value="dynamic">Re-solve each rebalance (dynamic)</option>
        </Select>
      </Field>

      {solving && (
        <>
          <Field label="Objective" htmlFor="weight-objective" hint={meta?.hint}>
            <Select
              id="weight-objective"
              value={optimizer.objective}
              onChange={(e) => setOptimizer({ objective: e.target.value as Objective })}
            >
              {OBJECTIVES.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </Field>

          {RETURN_DEPENDENT_OBJECTIVES.has(optimizer.objective) && (
            <p className="rounded-md border border-amber-700/40 bg-amber-950/30 px-3 py-2 text-xs text-amber-200/90">
              This objective depends on mean returns, which are far noisier to
              estimate than covariance. It optimizes the past Sharpe and often
              does not deliver a better one going forward.
            </p>
          )}

          {value.kind === "static" ? (
            <Field
              label="Warm-up (bars)"
              htmlFor="weight-warmup"
              hint="Observed before solving. Manual weights apply until then, so the solve cannot see ahead."
            >
              <BarsInput
                id="weight-warmup"
                min={2}
                value={value.warmup}
                onCommit={(warmup) => onChange({ ...value, warmup })}
              />
            </Field>
          ) : (
            <Field
              label="Lookback (bars)"
              htmlFor="weight-lookback"
              hint="Trailing window each re-solve sees. Solving starts once a full window exists."
            >
              <BarsInput
                id="weight-lookback"
                min={2}
                value={value.lookback}
                onCommit={(lookback) => onChange({ ...value, lookback })}
              />
            </Field>
          )}

          <Field
            label="Shrinkage"
            htmlFor="weight-shrinkage"
            hint="Pulls covariance toward its diagonal to damp noisy correlations. 0 = raw, 1 = ignore correlation."
          >
            <Input
              id="weight-shrinkage"
              type="number"
              min={0}
              max={1}
              step={0.05}
              value={optimizer.shrinkage}
              onChange={(e) =>
                setOptimizer({ shrinkage: Math.min(1, Math.max(0, Number(e.target.value))) })
              }
            />
          </Field>

          <Field
            label="Max position (%)"
            htmlFor="weight-cap"
            hint="Caps concentration. Blank = uncapped."
          >
            <Input
              id="weight-cap"
              type="number"
              min={1}
              max={100}
              step={5}
              value={optimizer.max_weight === null ? "" : Math.round(optimizer.max_weight * 100)}
              onChange={(e) => {
                const raw = e.target.value.trim();
                if (raw === "") return setOptimizer({ max_weight: null });
                const pct = Math.min(100, Math.max(1, Number(raw)));
                setOptimizer({ max_weight: pct / 100 });
              }}
            />
          </Field>

          {value.kind === "dynamic" && !hasRebalance && (
            <p className="text-xs text-slate-500">
              <StatusPill tone="warn" className="mr-1">
                note
              </StatusPill>
              Dynamic weights only change at rebalance dates. With no schedule set,
              monthly is used.
            </p>
          )}
        </>
      )}
    </div>
  );
}
