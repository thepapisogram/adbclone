import { useEffect, useMemo, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { Action, AppInfo } from "../../lib/types";
import { SafetyTag } from "../../components/SafetyTag";
import { candidates, defaultSelection, ramTotalKb, type Group, type Preset } from "./presets";

const presetInfo: Record<Preset, { title: string; text: string }> = {
  safe: { title: "Safe", text: "Preinstalled apps rated safe to remove. Recommended first step." },
  balanced: { title: "Balanced", text: "Also shows preinstalled apps rated \"caution\": they may be something you use. Nothing extra is ticked for you." },
  aggressive: { title: "Aggressive", text: "Also shows preinstalled apps nobody has rated yet. Only tick what you recognise. Apps rated unsafe are never offered." },
};
const groupInfo: Record<Group, string> = {
  safe: "Rated safe to remove",
  advanced: "Rated caution: check each one",
  unrated: "Not rated: your call",
};
const mb = (kb: number) => (kb >= 1_048_576 ? `${(kb / 1_048_576).toFixed(2)} GB` : `${Math.round(kb / 1024)} MB`);

interface Props {
  serial: string;
  apps: AppInfo[];
  busy: boolean;
  /** Opens the app's normal confirm dialog, which saves a safety snapshot before changing anything. */
  onApply: (action: Action, names: string[]) => void;
}

export function Debloat({ serial, apps, busy, onApply }: Props) {
  const [preset, setPreset] = useState<Preset>("safe");
  const [includeUser, setIncludeUser] = useState(false);
  const [method, setMethod] = useState<"disable" | "uninstall">("disable");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [ram, setRam] = useState<Map<string, number>>(new Map());
  const [ramNote, setRamNote] = useState("Measuring…");

  // Re-pick the defaults when the choices change, not when the list reloads after an apply.
  const list = useMemo(() => candidates(apps, preset, includeUser), [apps, preset, includeUser]);
  useEffect(() => { setSelected(defaultSelection(apps, preset, includeUser)); /* eslint-disable-next-line */ }, [preset, includeUser, serial, apps.length]);

  // One snapshot of what is running now, to estimate the RAM each app uses.
  useEffect(() => {
    if (!serial) return;
    let cancelled = false;
    setRamNote("Measuring…");
    ipc.listRunning(serial).then((r) => {
      if (cancelled) return;
      setRam(new Map(r.rows.filter((x) => x.package).map((x) => [x.package as string, x.rssKb])));
      setRamNote("");
    }).catch(() => !cancelled && setRamNote("RAM usage is not available."));
    return () => { cancelled = true; };
  }, [serial]);

  const toggle = (n: string) => setSelected((s) => { const x = new Set(s); x.has(n) ? x.delete(n) : x.add(n); return x; });
  const names = list.filter((c) => selected.has(c.app.name)).map((c) => c.app.name);
  const total = ramTotalKb(new Set(names), ram);
  const byGroup = (g: Group) => list.filter((c) => c.group === g).sort((a, b) => (ram.get(b.app.name) ?? 0) - (ram.get(a.app.name) ?? 0));

  if (!serial) return <div className="empty-state">Select a device to find apps you can remove.</div>;

  return (
    <div className="debloat">
      <div className="actions">
        <div className="presets" role="radiogroup" aria-label="Preset">
          {(Object.keys(presetInfo) as Preset[]).map((p) => (
            <button key={p} role="radio" aria-checked={preset === p} className={preset === p ? "active" : ""} onClick={() => setPreset(p)}>{presetInfo[p].title}</button>
          ))}
        </div>
        <label className="inline"><input type="checkbox" checked={includeUser} onChange={(e) => setIncludeUser(e.target.checked)} /> Include apps I installed</label>
        <div className="spacer" />
        <select value={method} onChange={(e) => setMethod(e.target.value as "disable" | "uninstall")} aria-label="Method">
          <option value="disable">Disable (easiest to undo)</option>
          <option value="uninstall">Uninstall for me</option>
        </select>
        <button className={method === "uninstall" ? "danger" : "primary"} disabled={busy || names.length === 0} onClick={() => onApply(method, names)}>
          {method === "disable" ? "Disable" : "Uninstall"} {names.length} app{names.length === 1 ? "" : "s"}
        </button>
      </div>
      <p className="note top">
        {presetInfo[preset].text} A snapshot is saved automatically before anything changes, and you can restore it from the Snapshots tab.
      </p>
      <div className="summary">
        <b>{names.length}</b> selected · RAM they use right now: <b>{ramNote || mb(total)}</b>
        {!ramNote && <span className="muted"> (an estimate: Android may restart some apps, and removing them mostly helps battery and background heat rather than free RAM)</span>}
      </div>
      <div className="tablewrap">
        {(["safe", "advanced", "unrated"] as Group[]).map((g) => {
          const rows = byGroup(g);
          if (!rows.length) return null;
          const allOn = rows.every((r) => selected.has(r.app.name));
          return (
            <section key={g} className="group">
              <h3>
                {groupInfo[g]} <span className="muted">({rows.length})</span>
                <button className="small" onClick={() => setSelected((s) => { const x = new Set(s); rows.forEach((r) => (allOn ? x.delete(r.app.name) : x.add(r.app.name))); return x; })}>
                  {allOn ? "Untick all" : "Tick all"}
                </button>
              </h3>
              <table>
                <tbody>
                  {rows.map(({ app }) => (
                    <tr key={app.name} className={selected.has(app.name) ? "sel" : ""} onClick={() => toggle(app.name)}>
                      <td className="chk"><input type="checkbox" checked={selected.has(app.name)} onChange={() => toggle(app.name)} onClick={(e) => e.stopPropagation()} aria-label={`Select ${app.label}`} /></td>
                      <td><div>{app.label}</div><div className="mono muted">{app.name}</div></td>
                      <td className="desc muted">{app.description ?? ""}</td>
                      <td className="num">{ram.has(app.name) ? mb(ram.get(app.name)!) : "not running"}</td>
                      <td><SafetyTag kind={method === "disable" ? "Disable" : "Uninstall"} value={method === "disable" ? app.disableSafety : app.uninstallSafety} /></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          );
        })}
        {list.length === 0 && <div className="empty">Nothing to suggest for this preset. Either it is already clean, or the names have not loaded yet.</div>}
      </div>
    </div>
  );
}
