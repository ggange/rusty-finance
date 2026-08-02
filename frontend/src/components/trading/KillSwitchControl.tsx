import { useState } from "react";
import { api } from "../../lib/apiClient";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Input } from "../ui/Input";
import { Panel } from "../ui/Panel";
import { StatusPill } from "../ui/StatusPill";
import type { KillSwitch } from "../../types/api";

/** Typing this exactly is required to release the halt. */
export const RELEASE_PHRASE = "RELEASE";

interface KillSwitchControlProps {
  killSwitch: KillSwitch | null;
  busy: boolean;
  act: <T>(fn: () => Promise<T>) => Promise<T | null>;
}

/**
 * Engage / release the manual halt.
 *
 * Deliberately asymmetric: engaging is one click, because stopping should never
 * be harder than starting. Releasing re-arms every code path that can place an
 * order, so it requires typing a confirmation phrase — the same reasoning the
 * API uses when it persists the halt across restarts.
 */
export function KillSwitchControl({ killSwitch, busy, act }: KillSwitchControlProps) {
  const [reason, setReason] = useState("");
  const [confirm, setConfirm] = useState("");

  const engaged = killSwitch?.engaged ?? false;
  const canRelease = confirm.trim().toUpperCase() === RELEASE_PHRASE;

  async function engage() {
    await act(() => api.trade.setKillSwitch(true, reason.trim() || null));
    setReason("");
  }

  async function release() {
    if (!canRelease) return;
    await act(() => api.trade.setKillSwitch(false, reason.trim() || null));
    setReason("");
    setConfirm("");
  }

  return (
    <Panel title="Kill switch">
      <div className="space-y-3">
        <div className="flex items-center justify-between gap-2">
          <StatusPill tone={engaged ? "bad" : "ok"}>
            {engaged ? "ENGAGED" : "released"}
          </StatusPill>
          {killSwitch?.updated_at && (
            <span className="text-xs text-slate-500">
              {new Date(killSwitch.updated_at).toLocaleString()}
            </span>
          )}
        </div>

        {engaged && killSwitch?.reason && (
          <p className="rounded-md bg-slate-900/60 px-3 py-2 text-xs text-slate-300">
            {killSwitch.reason}
          </p>
        )}

        <Field label="Reason" htmlFor="ks-reason" hint="Recorded in the audit log">
          <Input
            id="ks-reason"
            size="sm"
            value={reason}
            placeholder={engaged ? "why release?" : "why halt?"}
            onChange={(e) => setReason(e.target.value)}
          />
        </Field>

        {engaged ? (
          <>
            <Field
              label={`Type ${RELEASE_PHRASE} to confirm`}
              htmlFor="ks-confirm"
              hint="Releasing re-arms order submission, including entries"
            >
              <Input
                id="ks-confirm"
                size="sm"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
              />
            </Field>
            <Button
              variant="secondary"
              className="w-full"
              disabled={!canRelease || busy}
              loading={busy}
              onClick={() => void release()}
            >
              Release halt
            </Button>
          </>
        ) : (
          <Button
            variant="danger"
            className="w-full"
            disabled={busy}
            loading={busy}
            onClick={() => void engage()}
          >
            Engage kill switch
          </Button>
        )}

        <p className="text-xs text-slate-500">
          While engaged, no order of any side reaches the broker — exits included. The
          state is stored server-side and survives an API restart.
        </p>
      </div>
    </Panel>
  );
}
