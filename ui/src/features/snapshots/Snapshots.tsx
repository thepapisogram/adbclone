import { useCallback, useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import { SAFE_MODE, RESCUE_NOTE, latestSnapshot } from "./rescue";
import type { PlanResponse, RestoreResponse, SnapshotMeta } from "../../lib/types";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);
const when = (secs: number) => new Date(secs * 1000).toLocaleString();

interface Props {
  serial: string;
  /** Changes whenever another part of the app saves a snapshot, so the list reloads. */
  version: number;
  /** Called after a restore so the app list can reload. */
  onRestored: () => void;
}

export function Snapshots({ serial, version, onRestored }: Props) {
  const [list, setList] = useState<SnapshotMeta[]>([]);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const [restoring, setRestoring] = useState<SnapshotMeta | null>(null);
  const [confirmDelete, setConfirmDelete] = useState("");

  const latest = latestSnapshot(list);
  const load = useCallback(() => ipc.snapshots.list().then(setList).catch((e) => setMsg({ kind: "error", text: errText(e) })), []);
  useEffect(() => { load(); }, [load, version]);

  const guard = async (fn: () => Promise<void>) => {
    setBusy(true); setMsg(null);
    try { await fn(); } catch (e) { setMsg({ kind: "error", text: errText(e) }); } finally { setBusy(false); }
  };

  const take = () => guard(async () => {
    const m = await ipc.snapshots.create(serial, name.trim() || `Snapshot ${new Date().toLocaleString()}`, false);
    setName(""); setMsg({ kind: "ok", text: `Saved "${m.name}" (${m.enabled + m.disabled + m.uninstalled} packages).` }); await load();
  });

  const doImport = () => guard(async () => {
    const m = await ipc.snapshots.import();
    if (m) { setMsg({ kind: "ok", text: `Imported "${m.name}".` }); await load(); }
  });

  const doExport = (m: SnapshotMeta) => guard(async () => {
    if (await ipc.snapshots.export(m)) setMsg({ kind: "ok", text: `Exported "${m.name}".` });
  });

  return (
    <div className="snapshots">
      <div className="actions">
        <input type="text" className="grow" placeholder="Name this snapshot (optional)" value={name} maxLength={80}
          onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && serial && !busy && take()} />
        <button className="primary" disabled={!serial || busy} onClick={take}>Take snapshot now</button>
        <button disabled={busy} onClick={doImport}>Import file…</button>
      </div>
      {msg && <div className={`status ${msg.kind}`} role={msg.kind === "error" ? "alert" : "status"} style={{ margin: "8px 14px" }}>{msg.text}</div>}
      <details className="rescue" aria-label="Rescue mode">
        <summary><b>Phone acting up after removing apps? Rescue mode</b></summary>
        <ol>
          <li>
            <b>Put it back:</b>{" "}
            {latest ? <>the newest snapshot is "{latest.name}" ({when(latest.created)}).{" "}
              <button className="primary" disabled={!serial || busy} onClick={() => setRestoring(latest)}>Restore it…</button>
              {!serial && <span className="muted small"> Connect the phone first.</span>}</>
              : <span className="muted">no snapshot exists yet.</span>}
          </li>
          <li><b>If the phone is unusable or keeps restarting</b>, Safe Mode switches off apps you installed. Then connect it and restore as above.
            {SAFE_MODE.map((s) => (<div key={s.maker} className="saferow"><b>{s.maker}</b><ol>{s.steps.map((t, i) => <li key={i}>{t}</li>)}</ol></div>))}
          </li>
          <li className="muted small">{RESCUE_NOTE}</li>
        </ol>
      </details>
      <p className="note top">
        A snapshot records which apps are enabled, disabled or uninstalled. The app takes one automatically before every disable or uninstall batch
        (the newest 20 are kept), so you can always go back. Restoring never touches apps that are not in the snapshot.
      </p>
      <div className="tablewrap">
        <table>
          <thead><tr><th>Name</th><th>Taken</th><th>Device</th><th className="num">Enabled</th><th className="num">Disabled</th><th className="num">Uninstalled</th><th /></tr></thead>
          <tbody>
            {list.map((m) => (
              <tr key={m.id} className="static">
                <td>{m.name} {m.auto && <span className="pill">auto</span>}</td>
                <td>{when(m.created)}</td>
                <td>{m.deviceModel ?? "—"}</td>
                <td className="num">{m.enabled}</td><td className="num">{m.disabled}</td><td className="num">{m.uninstalled}</td>
                <td className="rowbtns">
                  <button disabled={!serial || busy} onClick={() => setRestoring(m)}>Restore…</button>
                  <button disabled={busy} onClick={() => doExport(m)}>Export</button>
                  {confirmDelete === m.id
                    ? <button className="danger" disabled={busy} onClick={() => guard(async () => { await ipc.snapshots.remove(m.id); setConfirmDelete(""); await load(); })}>Really delete</button>
                    : <button disabled={busy} onClick={() => setConfirmDelete(m.id)}>Delete</button>}
                </td>
              </tr>
            ))}
            {!list.length && <tr><td colSpan={7} className="empty">No snapshots yet. {serial ? "Take one before you start removing apps." : "Connect a device to take one."}</td></tr>}
          </tbody>
        </table>
      </div>
      {restoring && <RestoreDialog snap={restoring} serial={serial} onClose={(changed) => { setRestoring(null); if (changed) { onRestored(); load(); } }} />}
    </div>
  );
}

function RestoreDialog({ snap, serial, onClose }: { snap: SnapshotMeta; serial: string; onClose: (changed: boolean) => void }) {
  const [plan, setPlan] = useState<PlanResponse | null>(null);
  const [error, setError] = useState("");
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<RestoreResponse | null>(null);

  useEffect(() => { ipc.snapshots.plan(serial, snap.id).then(setPlan).catch((e) => setError(errText(e))); }, [serial, snap.id]);

  const go = async () => {
    setRunning(true); setError("");
    try { setResult(await ipc.snapshots.restore(serial, snap.id)); } catch (e) { setError(errText(e)); } finally { setRunning(false); }
  };

  const changes = plan?.plan.changes ?? [];
  const count = (step: string) => changes.filter((c) => c.steps[0] === step).length;
  const mismatch = plan && plan.snapshotModel && plan.deviceModel && plan.snapshotModel !== plan.deviceModel;
  const failed = result?.results.filter((r) => !r.ok) ?? [];

  return (
    <div className="modal-bg" onClick={() => !running && onClose(!!result)}>
      <div className="modal wide" role="dialog" aria-label="Restore snapshot" onClick={(e) => e.stopPropagation()}>
        <h2>Restore "{snap.name}"</h2>
        {error && <div className="status error" role="alert">{error}</div>}
        {!plan && !error && <p className="muted">Comparing with the phone…</p>}
        {plan && !result && (
          <>
            {mismatch && <p className="warn">This snapshot is from a different phone ({plan.snapshotModel}). Only packages that exist on this phone ({plan.deviceModel}) will change.</p>}
            {changes.length === 0
              ? <p>The phone already matches this snapshot. Nothing to change.</p>
              : <>
                  <p><b>{changes.length}</b> change{changes.length > 1 ? "s" : ""}: bring back {count("restore")}, enable {count("enable")}, disable {count("disable")}, uninstall {count("uninstall")}.
                    {plan.plan.unchanged > 0 && <> {plan.plan.unchanged} already match.</>}</p>
                  <ul className="changes">
                    {changes.slice(0, 40).map((c) => <li key={c.name}><span className="mono">{c.name}</span> <span className="muted">{c.from} → {c.to}</span></li>)}
                    {changes.length > 40 && <li className="muted">…and {changes.length - 40} more</li>}
                  </ul>
                  <p className="muted">A new automatic snapshot of the current state is saved first, so this restore can itself be undone.</p>
                </>}
            {plan.plan.missing.length > 0 && <p className="muted">{plan.plan.missing.length} package(s) in the snapshot are not on this phone and will be skipped.</p>}
          </>
        )}
        {result && (
          <>
            <p className={failed.length || result.remaining ? "warn" : "status ok"}>
              {result.results.length - failed.length} of {result.results.length} change(s) applied.
              {result.remaining === 0 ? " The phone now matches the snapshot." : ` ${result.remaining} still differ.`}
            </p>
            {failed.length > 0 && <ul className="changes">{failed.slice(0, 20).map((f) => <li key={f.name}><span className="mono">{f.name}</span> <span className="muted">{f.message}</span></li>)}</ul>}
            {result.undoSnapshot && <p className="muted">Saved "{result.undoSnapshot.name}" first, in case you want to go back.</p>}
          </>
        )}
        <div className="modal-actions">
          <button onClick={() => onClose(!!result)} disabled={running}>{result ? "Close" : "Cancel"}</button>
          {plan && !result && changes.length > 0 && <button className="primary" onClick={go} disabled={running}>{running ? "Restoring…" : `Restore ${changes.length} change${changes.length > 1 ? "s" : ""}`}</button>}
        </div>
      </div>
    </div>
  );
}
