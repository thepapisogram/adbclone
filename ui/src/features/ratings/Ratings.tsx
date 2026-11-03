import { useCallback, useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { AppInfo, RatingsOverview } from "../../lib/types";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);
const RISKS = [
  { id: "recommended", label: "Fine to remove" },
  { id: "advanced", label: "Only if you know what it is" },
  { id: "unsafe", label: "Never remove" },
] as const;

interface Props { apps: AppInfo[]; /** Called after ratings change so the app list is re-read with the new ratings. */ onChanged: () => void }

export function Ratings({ apps, onChanged }: Props) {
  const [ov, setOv] = useState<RatingsOverview>({ community: [], user: [] });
  const [msg, setMsg] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [pkg, setPkg] = useState("");
  const [risk, setRisk] = useState<(typeof RISKS)[number]["id"]>("advanced");
  const [note, setNote] = useState("");

  const load = useCallback(() => ipc.ratings.overview().then(setOv).catch((e) => setMsg({ kind: "error", text: errText(e) })), []);
  useEffect(() => { load(); }, [load]);

  const guard = async (fn: () => Promise<string | void>) => {
    setBusy(true); setMsg(null);
    try { const t = await fn(); if (t) setMsg({ kind: "ok", text: t }); await load(); onChanged(); } catch (e) { setMsg({ kind: "error", text: errText(e) }); } finally { setBusy(false); }
  };
  const choices = [...apps].sort((a, b) => a.label.localeCompare(b.label));

  return (
    <div className="storage">
      {msg && <div className={`status ${msg.kind}`} role={msg.kind === "error" ? "alert" : "status"}>{msg.text}</div>}

      <section className="block" aria-label="Rate an app">
        <h3>Rate an app yourself</h3>
        <p className="muted small">Your rating wins over the built-in one on this computer. Core system packages cannot be rated.</p>
        <div className="row">
          <select value={pkg} onChange={(e) => setPkg(e.target.value)} aria-label="App to rate">
            <option value="">Choose an app…</option>
            {choices.map((a) => <option key={a.name} value={a.name}>{a.label} ({a.name})</option>)}
          </select>
          <select value={risk} onChange={(e) => setRisk(e.target.value as typeof risk)} aria-label="Your rating">
            {RISKS.map((r) => <option key={r.id} value={r.id}>{r.label}</option>)}
          </select>
          <input type="text" className="grow" maxLength={160} placeholder="What it does, or why (optional)" value={note} onChange={(e) => setNote(e.target.value)} aria-label="Note" />
          <button className="primary" disabled={busy || !pkg} onClick={() => guard(async () => { await ipc.ratings.setUser(pkg, risk, note); setNote(""); return "Saved your rating."; })}>Save rating</button>
        </div>
        {ov.user.length > 0 && (
          <table className="storagetable" aria-label="Your ratings">
            <tbody>{ov.user.map((u) => (
              <tr key={u.id} className="static"><td className="mono">{u.id}</td><td>{RISKS.find((r) => r.id === u.risk)?.label}</td><td className="muted">{u.desc}</td>
                <td><button disabled={busy} onClick={() => guard(() => ipc.ratings.removeUser(u.id))}>Remove</button></td></tr>
            ))}</tbody>
          </table>
        )}
        <div className="row"><button disabled={busy || !ov.user.length} onClick={() => guard(async () => { const n = await ipc.ratings.exportUser(); return n == null ? undefined : `Saved ${n} rating${n === 1 ? "" : "s"} to a file you can share.`; })}>Export my ratings…</button></div>
      </section>

      <section className="block" aria-label="Shared ratings">
        <h3>Ratings shared by other people</h3>
        <p className="muted small">
          You can import a ratings file someone sent you. Be careful: nobody has checked it. To keep you safe, imported ratings are always treated as
          "only if you know what it is" (never "recommended"), are never ticked for you, can never change a built-in rating, and cannot touch core system apps or family rules.
          Marking something "never remove" is kept.
        </p>
        <div className="row"><button disabled={busy} onClick={() => guard(async () => { const r = await ipc.ratings.import(); return r ? `Imported ${r.accepted} rating${r.accepted === 1 ? "" : "s"} from ${r.file}${r.skipped ? ` (${r.skipped} skipped as unsafe or invalid)` : ""}.` : undefined; })}>Import a ratings file…</button></div>
        {ov.community.length === 0 ? <p className="muted small">Nothing imported.</p> : (
          <table className="storagetable" aria-label="Imported files">
            <tbody>{ov.community.map((c) => (
              <tr key={c.file} className="static"><td>{c.file}</td><td className="num">{c.entries} ratings</td><td><button disabled={busy} onClick={() => guard(() => ipc.ratings.removeCommunity(c.file))}>Remove</button></td></tr>
            ))}</tbody>
          </table>
        )}
      </section>
    </div>
  );
}
