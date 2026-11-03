import { useEffect, useMemo, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { Action, AppInfo } from "../../lib/types";
import { SafetyTag } from "../../components/SafetyTag";
import { AppIcon } from "../../components/AppIcon";
import { useSettings } from "../../lib/settings";
import { candidates, defaultSelection, protectedOffers, ramTotalKb, type Context, type Group, type Preset } from "./presets";

const presetInfo: Record<Preset, { title: string; text: string }> = {
  safe: { title: "Safe", text: "Preinstalled extras that nothing else depends on, and that we are confident about. These are ticked for you." },
  balanced: { title: "Balanced", text: "Also apps you might use. What you would lose is shown for each one, and nothing extra is ticked for you." },
  aggressive: { title: "Aggressive", text: "Also things other features may depend on, and apps nobody has rated yet. Only tick what you recognise." },
};
const groupInfo: Record<Group, string> = {
  safe: "Safe to remove",
  balanced: "Check each one: what you would lose is shown",
  aggressive: "May affect other features: be careful",
  unrated: "Not rated yet: your call",
};
const categoryName: Record<string, string> = {
  "ads-telemetry": "ads and reports", "partner-stub": "partner helper", "help-promo": "tips and promos", discontinued: "discontinued", "aosp-extra": "leftover extra",
  "game-hub": "games", assistant: "assistant", "vendor-app": "maker's app", "google-app": "Google app", "preinstalled-app": "preinstalled app",
  store: "app store", carrier: "carrier", "cloud-sync": "maker's cloud", payments: "payments", "system-service": "system service", "core-app": "main app",
  "background-service": "background helper", duplicate: "duplicate", other: "other",
};
const mb = (kb: number) => (kb >= 1_048_576 ? `${(kb / 1_048_576).toFixed(2)} GB` : `${Math.round(kb / 1024)} MB`);

interface Props {
  serial: string;
  apps: AppInfo[];
  busy: boolean;
  /** Apps the phone is using for its main jobs, with the reason in plain words. */
  protections: ReadonlyMap<string, string>;
  /** False when the phone could not tell us; main apps are then held back as a precaution. */
  protectionsKnown: boolean;
  /** Opens the app's normal confirm dialog, which saves a safety snapshot before changing anything. */
  onApply: (action: Action, names: string[]) => void;
}

export function Debloat({ serial, apps, busy, protections, protectionsKnown, onApply }: Props) {
  const { plain } = useSettings();
  const [preset, setPreset] = useState<Preset>("safe");
  const [includeUser, setIncludeUser] = useState(false);
  const [method, setMethod] = useState<"disable" | "uninstall">("disable");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [ram, setRam] = useState<Map<string, number>>(new Map());
  const [ramNote, setRamNote] = useState("Measuring…");

  const ctx: Context = useMemo(() => ({ includeUser, protectedPkgs: new Set(protections.keys()) }), [includeUser, protections]);
  // If the phone could not say what is in use, never offer a main app (calls, messages, camera, keyboard, browser, files, clock).
  const usable = useMemo(() => (protectionsKnown ? apps : apps.filter((a) => a.category !== "core-app")), [apps, protectionsKnown]);
  const list = useMemo(() => candidates(usable, preset, ctx), [usable, preset, ctx]);
  const held = useMemo(() => protectedOffers(apps, ctx, protections), [apps, ctx, protections]);
  // Re-pick the defaults when the choices change, not when the list reloads after an apply.
  useEffect(() => { setSelected(defaultSelection(usable, preset, ctx)); /* eslint-disable-next-line */ }, [preset, includeUser, serial, apps.length, protections]);

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
        Apps you use for calls, messages, the browser, the keyboard and the home screen are never offered.
      </p>
      {!protectionsKnown && <div className="error" role="alert">Could not check which apps you use for main jobs, so main apps are hidden here to be safe.</div>}
      <div className="summary">
        <b>{names.length}</b> selected · RAM they use right now: <b>{ramNote || mb(total)}</b>
        {!ramNote && <span className="muted"> (an estimate: Android may restart some apps, and removing them mostly helps battery and background heat rather than free RAM)</span>}
      </div>
      <div className="tablewrap">
        {(["safe", "balanced", "aggressive", "unrated"] as Group[]).map((g) => {
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
                    <tr key={app.name} className={selected.has(app.name) ? "sel" : ""} onClick={() => toggle(app.name)} title={app.description ?? undefined}>
                      <td className="chk"><input type="checkbox" checked={selected.has(app.name)} onChange={() => toggle(app.name)} onClick={(e) => e.stopPropagation()} aria-label={`Select ${app.label}`} /></td>
                      <td><div className="namecell"><AppIcon name={app.name} /><div><div>{app.label}</div>{!plain && <div className="mono muted">{app.name}</div>}</div></div></td>
                      <td className="desc">
                        <div>{app.impact ?? (g === "unrated" ? "Nobody has rated this app yet. Only remove it if you know what it is." : app.description) ?? ""}</div>
                        <div className="muted small">
                          {app.category && categoryName[app.category] && <span className="pill">{categoryName[app.category]}</span>}{" "}
                          {app.confidence !== "high" && g !== "unrated" && <span className="pill" title="We are less sure about this rating, so it is listed under a more careful group.">less sure</span>}{" "}
                          {app.reinstall === "store" ? "Reinstall from the store." : app.reinstall === "rom-only" ? "Undo from a snapshot." : ""}
                        </div>
                      </td>
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
        {held.length > 0 && (
          <details className="group heldback">
            <summary>Not offered because you use them ({held.length})</summary>
            <ul aria-label="Apps you use">
              {held.map(({ app, reason }) => <li key={app.name}><span className="namecell"><AppIcon name={app.name} size={20} />{app.label}</span> <span className="muted">is {reason}</span></li>)}
            </ul>
          </details>
        )}
      </div>
    </div>
  );
}
