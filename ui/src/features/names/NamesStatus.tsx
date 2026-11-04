import { useState } from "react";
import { ipc } from "../../lib/ipc";
import type { CheckReport, LabelProgress, NamesStatusInfo } from "../../lib/types";

interface Props {
  status: NamesStatusInfo | null;
  progress: LabelProgress | null;
  onRetry: () => void;
  nameOf: (pkg: string) => string;
  serial?: string;
}

/** One line saying how app names were found, with a panel explaining any that were not. */
export function NamesStatus({ status, progress, onRetry, nameOf, serial }: Props) {
  const [open, setOpen] = useState(false);
  const [check, setCheck] = useState<CheckReport | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkError, setCheckError] = useState("");
  const [copied, setCopied] = useState(false);
  const runCheck = async () => {
    if (!serial) return;
    setChecking(true); setCheck(null); setCheckError(""); setCopied(false);
    try { setCheck(await ipc.bridgeCheck(serial)); } catch (e) { setCheckError(String(e instanceof Error ? e.message : e)); } finally { setChecking(false); }
  };
  const checkText = check ? [check.verdict, ...check.steps.map((s) => `[${s.ok ? "ok" : "FAILED"}] ${s.name}: ${s.detail}`)].join("\n") : "";
  const loading = !status?.finished;
  const summary = status
    ? `Names: ${status.helper} from the phone${status.file + status.cached > 0 ? ` · ${status.file + status.cached} from app files` : ""}${status.unresolved.length ? ` · ${status.unresolved.length} unknown` : ""}`
    : progress && progress.done < progress.total ? `Finding names ${progress.done}/${progress.total}` : "Finding names…";
  return (
    <>
      <button className="linklike small" onClick={() => setOpen(true)} title="How the app names were found">{summary}{loading && status ? " …" : ""}</button>
      {open && (
        <div className="modal-bg" onClick={() => setOpen(false)}>
          <div className="modal wide" role="dialog" aria-label="App names" onClick={(e) => e.stopPropagation()}>
            <h2>App names</h2>
            {status?.helperMessage
              ? <p className="warn">The on-device helper could not be used: {status.helperMessage}. Names were read from the app files instead, which is slower and can miss some.</p>
              : status && status.helper > 0 ? <p className="muted small">Names came straight from the phone through a temporary helper that is removed again after use. Nothing is installed.</p> : null}
            {status && status.iconsHelper !== undefined && <p>Icons: {status.iconsHelper} drawn by the phone · {status.iconsFile} read from app files · {status.iconsMissing} without one</p>}
            {status && <p>{status.helper} from the phone · {status.file} read from app files · {status.cached} remembered from before · {status.unresolved.length} unknown</p>}
            {status && status.unresolved.length > 0 && (
              <ul className="changes" aria-label="Apps without a name">
                {status.unresolved.slice(0, 80).map((u) => <li key={u.name}><span className="mono">{u.name}</span> <span className="muted">{nameOf(u.name) !== u.name ? "" : ""}{u.reason}</span></li>)}
                {status.unresolved.length > 80 && <li className="muted">…and {status.unresolved.length - 80} more</li>}
              </ul>
            )}
            {status && status.unresolved.length === 0 && status.finished && <p className="status ok">Every app has a name.</p>}
            <h3>Check app info access</h3>
            <p className="muted small">Tests, step by step, whether Coolant can read names, icons and details from this phone. Changes nothing (a 5 KB temporary file is deleted at the end).</p>
            <div className="row"><button disabled={!serial || checking} onClick={runCheck}>{checking ? "Checking…" : "Run the check"}</button></div>
            {checkError && <p className="status error" role="alert">{checkError}</p>}
            {check && (
              <div aria-label="Check result">
                <p className={`status ${check.works ? "ok" : "error"}`}>{check.verdict}</p>
                <ul className="steps-list">{check.steps.map((s) => <li key={s.name} className={s.ok ? "ok" : "bad"}>{s.ok ? "✓" : "✗"} {s.name}<span className="muted"> — {s.detail}</span></li>)}</ul>
                <button onClick={() => { navigator.clipboard?.writeText(checkText).then(() => setCopied(true)).catch(() => {}); }}>{copied ? "Copied" : "Copy this result"}</button>
              </div>
            )}
            <div className="modal-actions">
              <button onClick={() => setOpen(false)}>Close</button>
              <button className="primary" onClick={() => { onRetry(); setOpen(false); }}>Find names again</button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
