import type { ReactNode } from "react";

export type Tone = "ok" | "warn" | "bad" | "neutral" | "info";

const TONES: Record<Tone, string> = {
  ok: "bg-emerald-500/15 text-emerald-300 border-emerald-600/40",
  warn: "bg-amber-500/15 text-amber-300 border-amber-600/40",
  bad: "bg-rose-500/15 text-rose-300 border-rose-600/40",
  info: "bg-sky-500/15 text-sky-300 border-sky-600/40",
  neutral: "bg-slate-700/40 text-slate-400 border-slate-600",
};

interface StatusPillProps {
  tone?: Tone;
  children: ReactNode;
  className?: string;
}

export function StatusPill({ tone = "neutral", children, className = "" }: StatusPillProps) {
  return (
    <span
      className={`inline-block rounded-full border px-3 py-1 text-xs font-medium ${TONES[tone]} ${className}`}
    >
      {children}
    </span>
  );
}

/** Maps a broker/venue order status onto a pill tone. */
export function orderTone(status: string): Tone {
  switch (status) {
    case "filled":
      return "ok";
    case "accepted":
    case "partially_filled":
      return "info";
    case "rejected":
      return "bad";
    case "canceled":
      return "neutral";
    default:
      return "neutral";
  }
}
