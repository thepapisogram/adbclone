import { useState } from "react";
import type { LabelProgress, NamesStatusInfo } from "../../lib/types";

interface Props {
  status: NamesStatusInfo | null;
  progress: LabelProgress | null;
  onRetry: () => void;
  nameOf: (pkg: string) => string;
}

/** One line saying how app names were found, with a panel explaining any that were not. */
export function NamesStatus({ status, progress, onRetry, nameOf }: Props) {
  const [open, setOpen] = useState(false);
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
            {status && <p>{status.helper} from the phone · {status.file} read from app files · {status.cached} remembered from before · {status.unresolved.length} unknown</p>}
            {status && status.unresolved.length > 0 && (
              <ul className="changes" aria-label="Apps without a name">
                {status.unresolved.slice(0, 80).map((u) => <li key={u.name}><span className="mono">{u.name}</span> <span className="muted">{nameOf(u.name) !== u.name ? "" : ""}{u.reason}</span></li>)}
                {status.unresolved.length > 80 && <li className="muted">…and {status.unresolved.length - 80} more</li>}
              </ul>
            )}
            {status && status.unresolved.length === 0 && status.finished && <p className="status ok">Every app has a name.</p>}
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
