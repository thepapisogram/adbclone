import { useCallback, useEffect, useMemo, useState } from "react";
import { ipc } from "./lib/ipc";
import type { Action, AppInfo, Device, PkgState, Risk } from "./lib/types";

type Scope = "all" | "system" | "user";
type StateFilter = "any" | PkgState;
interface LogEntry { time: string; action: Action; name: string; ok: boolean; message?: string; prev: PkgState }

const riskOrder: Record<Risk, number> = { recommended: 0, advanced: 1, unknown: 2, unsafe: 3 };
const undoOf: Record<Action, Action> = { disable: "enable", enable: "disable", uninstall: "restore", restore: "uninstall" };
const labelOf: Record<Action, string> = { disable: "Disable", enable: "Enable", uninstall: "Uninstall", restore: "Restore" };

export default function App() {
  const [devices, setDevices] = useState<Device[]>([]);
  const [serial, setSerial] = useState("");
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<Scope>("all");
  const [stateFilter, setStateFilter] = useState<StateFilter>("any");
  const [riskFilter, setRiskFilter] = useState<Risk | "any">("any");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [sort, setSort] = useState<{ key: "label" | "name" | "risk"; dir: 1 | -1 }>({ key: "label", dir: 1 });
  const [pending, setPending] = useState<{ action: Action; names: string[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const [log, setLog] = useState<LogEntry[]>([]);
  const [showLog, setShowLog] = useState(false);

  const refreshDevices = useCallback(async () => {
    try {
      const d = await ipc.listDevices();
      setDevices(d);
      setSerial((cur) => cur || d.find((x) => x.state === "device")?.serial || "");
      setError("");
    } catch (e) { setError(String(e)); }
  }, []);

  const loadApps = useCallback(async (s: string) => {
    if (!s) { setApps([]); return; }
    setLoading(true);
    try { setApps(await ipc.listApps(s)); setSelected(new Set()); setError(""); }
    catch (e) { setError(String(e)); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => { refreshDevices(); }, [refreshDevices]);
  useEffect(() => { loadApps(serial); }, [serial, loadApps]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = apps.filter((a) =>
      (scope === "all" || (scope === "system") === a.system) &&
      (stateFilter === "any" || a.state === stateFilter) &&
      (riskFilter === "any" || a.risk === riskFilter) &&
      (!q || a.name.toLowerCase().includes(q) || a.label.toLowerCase().includes(q)));
    list.sort((a, b) => {
      const c = sort.key === "risk" ? riskOrder[a.risk] - riskOrder[b.risk] : a[sort.key].localeCompare(b[sort.key]);
      return c * sort.dir;
    });
    return list;
  }, [apps, query, scope, stateFilter, riskFilter, sort]);

  const toggle = (name: string) => setSelected((s) => { const n = new Set(s); n.has(name) ? n.delete(name) : n.add(name); return n; });
  const allVisibleSelected = visible.length > 0 && visible.every((a) => selected.has(a.name));
  const toggleAll = () => setSelected(allVisibleSelected ? new Set() : new Set(visible.map((a) => a.name)));
  const sortBy = (key: typeof sort.key) => setSort((s) => ({ key, dir: s.key === key ? (-s.dir as 1 | -1) : 1 }));

  const selectedApps = apps.filter((a) => selected.has(a.name));
  const unsafeCount = (names: string[]) => apps.filter((a) => names.includes(a.name) && a.risk === "unsafe").length;

  async function run(action: Action, names: string[]) {
    setBusy(true);
    const prevs = new Map(apps.map((a) => [a.name, a.state]));
    try {
      const results = await ipc.applyAction(serial, action, names);
      const time = new Date().toLocaleTimeString();
      setLog((l) => [...results.map((r) => ({ time, action, name: r.name, ok: r.ok, message: r.message, prev: prevs.get(r.name) ?? "enabled" })), ...l]);
      await loadApps(serial);
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); setPending(null); }
  }

  const eligible = (action: Action) => selectedApps.filter((a) =>
    action === "disable" ? a.state === "enabled" : action === "enable" ? a.state === "disabled" :
    action === "uninstall" ? a.state !== "uninstalled" : a.state === "uninstalled").map((a) => a.name);

  const device = devices.find((d) => d.serial === serial);
  const counts = { total: apps.length, disabled: apps.filter((a) => a.state === "disabled").length, removed: apps.filter((a) => a.state === "uninstalled").length };

  return (
    <div className="app">
      <header>
        <h1>App Manager</h1>
        {ipc.isMock && <span className="badge mock" title="Running in a browser without the Rust backend">mock data</span>}
        <div className="spacer" />
        <select value={serial} onChange={(e) => setSerial(e.target.value)} aria-label="Device">
          <option value="">{devices.length ? "Select device…" : "No devices"}</option>
          {devices.map((d) => (
            <option key={d.serial} value={d.serial} disabled={d.state !== "device"}>
              {d.model ?? d.serial} — {d.state === "device" ? d.serial : d.state}
            </option>
          ))}
        </select>
        <button onClick={refreshDevices}>Refresh devices</button>
      </header>

      {error && <div className="error" role="alert">{error}</div>}
      {device?.state === "unauthorized" && <div className="error">Accept the USB debugging prompt on the phone.</div>}

      <div className="toolbar">
        <input type="search" placeholder="Search name or package…" value={query} onChange={(e) => setQuery(e.target.value)} />
        <select value={scope} onChange={(e) => setScope(e.target.value as Scope)} aria-label="Type">
          <option value="all">All apps</option><option value="system">System</option><option value="user">User</option>
        </select>
        <select value={stateFilter} onChange={(e) => setStateFilter(e.target.value as StateFilter)} aria-label="State">
          <option value="any">Any state</option><option value="enabled">Enabled</option><option value="disabled">Disabled</option><option value="uninstalled">Uninstalled</option>
        </select>
        <select value={riskFilter} onChange={(e) => setRiskFilter(e.target.value as Risk | "any")} aria-label="Risk">
          <option value="any">Any risk</option><option value="recommended">Recommended</option><option value="advanced">Advanced</option><option value="unsafe">Unsafe</option><option value="unknown">Unrated</option>
        </select>
        <button onClick={() => loadApps(serial)} disabled={!serial || loading}>{loading ? "Loading…" : "Reload"}</button>
      </div>

      <div className="actions">
        <span>{selected.size} selected · {visible.length} shown · {counts.total} total · {counts.disabled} disabled · {counts.removed} uninstalled</span>
        <div className="spacer" />
        {(["disable", "enable", "uninstall", "restore"] as Action[]).map((a) => (
          <button key={a} className={a === "uninstall" ? "danger" : ""} disabled={busy || eligible(a).length === 0}
            onClick={() => setPending({ action: a, names: eligible(a) })}>
            {labelOf[a]} ({eligible(a).length})
          </button>
        ))}
        <button onClick={() => setShowLog((v) => !v)}>History ({log.length})</button>
      </div>

      <div className="content">
        <div className="tablewrap">
          <table>
            <thead>
              <tr>
                <th className="chk"><input type="checkbox" checked={allVisibleSelected} onChange={toggleAll} aria-label="Select all" /></th>
                <th onClick={() => sortBy("label")}>App {sort.key === "label" && (sort.dir === 1 ? "▲" : "▼")}</th>
                <th onClick={() => sortBy("name")}>Package {sort.key === "name" && (sort.dir === 1 ? "▲" : "▼")}</th>
                <th>Type</th><th>State</th>
                <th onClick={() => sortBy("risk")}>Risk {sort.key === "risk" && (sort.dir === 1 ? "▲" : "▼")}</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((a) => (
                <tr key={a.name} className={selected.has(a.name) ? "sel" : ""} onClick={() => toggle(a.name)} title={a.description}>
                  <td className="chk"><input type="checkbox" checked={selected.has(a.name)} onChange={() => toggle(a.name)} onClick={(e) => e.stopPropagation()} /></td>
                  <td>{a.label}</td>
                  <td className="mono">{a.name}</td>
                  <td>{a.system ? "System" : "User"}</td>
                  <td><span className={`pill ${a.state}`}>{a.state}</span></td>
                  <td><span className={`pill risk-${a.risk}`}>{a.risk === "unknown" ? "unrated" : a.risk}</span></td>
                </tr>
              ))}
              {!visible.length && <tr><td colSpan={6} className="empty">{serial ? (loading ? "Loading apps…" : "No apps match.") : "Connect a device with USB debugging enabled and select it above."}</td></tr>}
            </tbody>
          </table>
        </div>

        {showLog && (
          <aside className="log">
            <h2>History</h2>
            {!log.length && <p className="muted">Nothing yet.</p>}
            {log.map((e, i) => (
              <div key={i} className={`entry ${e.ok ? "" : "fail"}`}>
                <div><b>{labelOf[e.action]}</b> <span className="mono">{e.name}</span></div>
                <div className="muted">{e.time} · {e.ok ? "ok" : e.message ?? "failed"}</div>
                {e.ok && <button disabled={busy} onClick={() => run(undoOf[e.action], [e.name])}>Undo</button>}
              </div>
            ))}
          </aside>
        )}
      </div>

      {pending && (
        <div className="modal-bg" onClick={() => !busy && setPending(null)}>
          <div className="modal" role="dialog" onClick={(e) => e.stopPropagation()}>
            <h2>{labelOf[pending.action]} {pending.names.length} app{pending.names.length > 1 ? "s" : ""}?</h2>
            {unsafeCount(pending.names) > 0 && pending.action !== "enable" && pending.action !== "restore" && (
              <p className="warn">⚠ {unsafeCount(pending.names)} selected app(s) are rated <b>unsafe</b>. Removing core system packages can cause a bootloop.</p>
            )}
            <ul>{pending.names.slice(0, 8).map((n) => <li key={n} className="mono">{n}</li>)}{pending.names.length > 8 && <li>…and {pending.names.length - 8} more</li>}</ul>
            <p className="muted">{pending.action === "uninstall" ? "Removed for the current user only; can be restored later." : "Reversible from the History panel."}</p>
            <div className="modal-actions">
              <button onClick={() => setPending(null)} disabled={busy}>Cancel</button>
              <button className={pending.action === "uninstall" ? "danger" : "primary"} disabled={busy} onClick={() => run(pending.action, pending.names)}>{busy ? "Working…" : labelOf[pending.action]}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
