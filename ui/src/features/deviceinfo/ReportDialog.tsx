import { useEffect, useRef, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { ReportOut } from "../../lib/types";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Collects a read-only report of the phone's raw command output so it can be shared and used to fix parsers. */
export function ReportDialog({ serial, onClose }: { serial: string; onClose: () => void }) {
  const [report, setReport] = useState<ReportOut | null>(null);
  const [error, setError] = useState("");
  const [msg, setMsg] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    let cancelled = false;
    ipc.report.collect(serial).then((r) => !cancelled && setReport(r)).catch((e) => !cancelled && setError(errText(e)));
    return () => { cancelled = true; };
  }, [serial]);

  const copy = async () => {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(report.text);
      setMsg({ kind: "ok", text: "Copied. Paste it into your message." });
    } catch {
      // Clipboard access can be refused: fall back to selecting the text so Ctrl+C works.
      area.current?.focus(); area.current?.select();
      setMsg({ kind: "error", text: "Could not copy automatically. The text is selected: press Ctrl+C." });
    }
  };
  const save = async () => {
    if (!report) return;
    try { setMsg((await ipc.report.save(report.text)) ? { kind: "ok", text: "Saved." } : null); }
    catch (e) { setMsg({ kind: "error", text: errText(e) }); }
  };

  const failed = report?.sections.filter((s) => !s.ok).length ?? 0;
  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal wide" role="dialog" aria-label="Device report" onClick={(e) => e.stopPropagation()}>
        <h2>Device report</h2>
        <p className="muted small">
          Collects what this app reads from your phone (temperatures, battery, processes, settings) exactly as the phone prints it, so problems can be fixed from real data.
          It only reads: nothing on the phone is changed. Serial numbers, e-mail addresses, IP and MAC addresses and long ID numbers are replaced with placeholders.
          It lists preinstalled apps but not the apps you installed. Read it before you share it.
        </p>
        {!report && !error && <p className="status busy">Collecting… this takes a few seconds.</p>}
        {error && <div className="status error" role="alert">{error}</div>}
        {report && (
          <>
            <p>
              {report.sections.length - failed} of {report.sections.length} parts collected
              {failed > 0 && <span className="muted"> ({failed} not available on this phone; that is still useful to know)</span>}.
            </p>
            <textarea ref={area} className="reportbox" readOnly value={report.text} aria-label="Report text" />
          </>
        )}
        {msg && <div className={`status ${msg.kind}`} role={msg.kind === "error" ? "alert" : "status"}>{msg.text}</div>}
        <div className="modal-actions">
          <button onClick={onClose}>Close</button>
          <button disabled={!report} onClick={copy}>Copy</button>
          <button className="primary" disabled={!report} onClick={save}>Save as file…</button>
        </div>
      </div>
    </div>
  );
}
