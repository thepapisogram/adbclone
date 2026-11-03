import { useEffect, useMemo, useRef, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { CommandLogEntry } from "../../lib/types";

const time = (ms: number) => new Date(ms).toLocaleTimeString();

/** Every command the app sends to the phone, so nothing happens out of sight. */
export function Logs({ active }: { active: boolean }) {
  const [entries, setEntries] = useState<CommandLogEntry[]>([]);
  const [paused, setPaused] = useState(false);
  const [onlyProblems, setOnlyProblems] = useState(false);
  const [copied, setCopied] = useState("");
  const seq = useRef(0);

  useEffect(() => {
    if (!active || paused) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const loop = async () => {
      try {
        const r = await ipc.log.since(seq.current);
        if (!stopped && r.entries.length) {
          seq.current = r.seq;
          setEntries((prev) => {
            // New entries append; an entry that was updated (same id) replaces its older copy.
            const byId = new Map(prev.map((e) => [e.id, e]));
            for (const e of r.entries) byId.set(e.id, e);
            return [...byId.values()].sort((a, b) => a.id - b.id).slice(-500);
          });
        } else if (!stopped) seq.current = r.seq;
      } catch { /* nothing to show */ }
      if (!stopped) timer = setTimeout(loop, 1500);
    };
    loop();
    return () => { stopped = true; clearTimeout(timer); };
  }, [active, paused]);

  const shown = useMemo(() => [...entries].reverse().filter((e) => !onlyProblems || !e.ok), [entries, onlyProblems]);
  const asText = () => shown.map((e) => `${time(e.timeMs)}  ${e.ok ? "ok  " : "FAIL"}  ${e.command}${e.count > 1 ? `  (x${e.count})` : ""}${e.note ? `  -- ${e.note}` : ""}`).join("\n");

  return (
    <div className="logs">
      <div className="actions">
        <span className="muted small">Every command sent to your phone, newest first. Background checks that repeat are counted (×N). Pairing codes are hidden. Lines can include your phone's serial number.</span>
        <div className="spacer" />
        <label className="inline"><input type="checkbox" checked={onlyProblems} onChange={(e) => setOnlyProblems(e.target.checked)} /> Only problems</label>
        <button onClick={() => setPaused((p) => !p)}>{paused ? "Resume" : "Pause"}</button>
        <button onClick={async () => { try { await navigator.clipboard.writeText(asText()); setCopied("Copied."); } catch { setCopied("Could not copy."); } setTimeout(() => setCopied(""), 2500); }}>Copy</button>
        <button onClick={async () => { await ipc.log.clear(); setEntries([]); }}>Clear</button>
        {copied && <span className="muted small">{copied}</span>}
      </div>
      <div className="tablewrap">
        <table className="logtable">
          <thead><tr><th>Time</th><th /><th>Command</th><th className="num">ms</th></tr></thead>
          <tbody>
            {shown.map((e) => (
              <tr key={e.id} className="static">
                <td className="muted">{time(e.timeMs)}</td>
                <td className={e.ok ? "ok-mark" : "bad-mark"} aria-label={e.ok ? "worked" : "failed"}>{e.ok ? "✓" : "✗"}</td>
                <td className="mono">{e.command}{e.count > 1 && <span className="muted"> ×{e.count}</span>}{e.note && <div className="muted">{e.note}</div>}</td>
                <td className="num">{e.millis}</td>
              </tr>
            ))}
            {!shown.length && <tr><td colSpan={4} className="empty">{onlyProblems ? "No problems so far." : "Nothing yet. Commands appear here as the app talks to your phone."}</td></tr>}
          </tbody>
        </table>
      </div>
    </div>
  );
}
