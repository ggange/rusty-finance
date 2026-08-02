import { StatusPill } from "../ui/StatusPill";
import type { Tone } from "../ui/StatusPill";
import type {
  BrokerInfo,
  KillSwitch,
  LimitsForPlan,
  ReconcileResult,
  ScheduleStatus,
} from "../../types/api";

function formatTime(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function Stat({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="min-w-0">
      <p className="text-[10px] uppercase tracking-wide text-slate-500">{label}</p>
      <div className="mt-1 truncate text-sm text-slate-200">{children}</div>
    </div>
  );
}

function Banner({ tone, children }: { tone: Tone; children: React.ReactNode }) {
  const cls =
    tone === "bad"
      ? "border-rose-600/50 bg-rose-950/50 text-rose-200"
      : "border-amber-600/50 bg-amber-950/40 text-amber-200";
  return (
    <div className={`rounded-lg border px-4 py-3 text-sm font-medium ${cls}`}>{children}</div>
  );
}

interface StatusStripProps {
  broker: BrokerInfo | null;
  schedule: ScheduleStatus | null;
  killSwitch: KillSwitch | null;
  limits: LimitsForPlan | null;
  reconcile: ReconcileResult | null;
}

/**
 * The always-visible health band. Two conditions get a loud banner rather than
 * a pill, because they mean capital is at risk in a way a glance should catch:
 * an engaged kill switch, and a live broker running without limits. These
 * mirror the warnings the API already logs at boot.
 */
export function StatusStrip({
  broker,
  schedule,
  killSwitch,
  limits,
  reconcile,
}: StatusStripProps) {
  const live = broker?.is_live ?? false;
  const eff = limits?.effective;
  // The API fails closed on live brokers when these two are unset (api/risk.py).
  const missingRequiredLimits =
    live && (eff?.max_position_value == null || eff?.max_daily_loss == null);

  return (
    <div className="space-y-3">
      {killSwitch?.engaged && (
        <Banner tone="bad">
          Kill switch is ENGAGED — no orders will reach the broker, including exits.
          {killSwitch.reason ? ` Reason: ${killSwitch.reason}` : ""}
        </Banner>
      )}
      {missingRequiredLimits && (
        <Banner tone="warn">
          Live broker with no global position/loss limits set. The API fails closed, so
          entries will be rejected until limits exist.
        </Banner>
      )}

      <div className="grid gap-4 rounded-xl border border-slate-700 bg-slate-800/50 p-4 sm:grid-cols-2 lg:grid-cols-5">
        <Stat label="Broker">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{broker?.name ?? "…"}</span>
            <StatusPill tone={live ? "bad" : "neutral"}>
              {live ? "LIVE" : "paper"}
            </StatusPill>
          </div>
        </Stat>

        <Stat label="Simulated fills">
          {broker
            ? `${(broker.slippage * 10_000).toFixed(0)} bps · ${(broker.fill_ratio * 100).toFixed(0)}% fill`
            : "—"}
        </Stat>

        <Stat label="Scheduler">
          <div className="flex flex-wrap items-center gap-2">
            <StatusPill tone={schedule?.running ? "ok" : "neutral"}>
              {schedule ? (schedule.running ? "running" : "stopped") : "…"}
            </StatusPill>
            {schedule?.cron && (
              <span className="text-xs text-slate-400">
                {String(schedule.cron.hour).padStart(2, "0")}:
                {String(schedule.cron.minute).padStart(2, "0")} {schedule.cron.timezone}
              </span>
            )}
          </div>
        </Stat>

        <Stat label="Next / last run">
          <span className="text-xs text-slate-300">
            {formatTime(schedule?.next_run ?? null)}
            <span className="text-slate-600"> · </span>
            {formatTime(schedule?.last_run?.created_at ?? null)}
          </span>
        </Stat>

        <Stat label="Reconciliation">
          {reconcile ? (
            <StatusPill tone={reconcile.in_sync ? "ok" : "bad"}>
              {reconcile.in_sync
                ? `in sync · ${reconcile.checked} symbol${reconcile.checked === 1 ? "" : "s"}`
                : `${reconcile.drift.length} drifting`}
            </StatusPill>
          ) : (
            <StatusPill>…</StatusPill>
          )}
        </Stat>
      </div>
    </div>
  );
}
