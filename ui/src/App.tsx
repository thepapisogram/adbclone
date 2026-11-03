import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ipc } from "./lib/ipc";
import { BRAND } from "./brand";
import { updateSettings, useSettings } from "./lib/settings";
import { reasonOf } from "./lib/reason";
import { categoryCounts, duplicateTarget, INSTALLER_OPTIONS, matchesFilters, NO_FILTERS, type Filters, type InstallerKey } from "./lib/filters";
import { nameOfCategory } from "./lib/categories";
import { SettingsDialog } from "./components/SettingsDialog";
import { ConnectDialog } from "./features/connect/ConnectDialog";
import { isWireless } from "./lib/types";
import { pickSerial } from "./lib/pickSerial";
import type { Protection, AdbStatus, Action, AppInfo, Device, LabelProgress, NamesStatusInfo, NameSource, PkgState, Risk } from "./lib/types";
import { addIcons, clearIcons } from "./lib/icons";
import { AppIcon } from "./components/AppIcon";
import { NamesStatus } from "./features/names/NamesStatus";
import { SafetyTag } from "./components/SafetyTag";
import { Running } from "./features/running/Running";
import { Snapshots } from "./features/snapshots/Snapshots";
import { Health } from "./features/health/Health";
import { Debloat } from "./features/debloat/Debloat";
import { Tune } from "./features/tuning/Tune";
import { Logs } from "./features/logs/Logs";
import { AdbSetup } from "./features/setup/AdbSetup";
import { Guide } from "./features/guide/Guide";
import { Storage } from "./features/storage/Storage";
import { Wake } from "./features/wake/Wake";
import { useT } from "./lib/i18n";
import { Phones } from "./features/phones/Phones";
import { appliesTo, otherReady, repeatable } from "./lib/multi";
import { Ratings } from "./features/ratings/Ratings";
import { InstallAndBackup } from "./features/install/Install";
import { DeviceCard } from "./features/deviceinfo/DeviceCard";

type Scope = "all" | "system" | "user";
type StateFilter = "any" | PkgState;
interface LogEntry { time: string; serial?: string; action: Action; name: string; ok: boolean; message?: string; prev: PkgState }

const riskOrder: Record<Risk, number> = { recommended: 0, advanced: 1, unknown: 2, unsafe: 3 };
const undoOf: Partial<Record<Action, Action>> = { disable: "enable", enable: "disable", uninstall: "restore", restore: "uninstall" };


export default function App() {
  const t = useT();
  const labelOf: Record<Action, string> = { disable: t("action.disable"), enable: t("action.enable"), uninstall: t("action.uninstall"), restore: t("action.restore"), stop: t("action.stop") };
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
  const [tab, setTab] = useState<"apps" | "running" | "health" | "debloat" | "tune" | "install" | "storage" | "wake" | "phones" | "ratings" | "snapshots" | "logs">("apps");
  const [snapVersion, setSnapVersion] = useState(0);
  const [notice, setNotice] = useState("");
  const historyReady = useRef(false);
  const connectOpen = useRef(false);
  const [showConnect, setShowConnect] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [filters, setFilters] = useState<Filters>(NO_FILTERS);
  // Apps the phone is using for main jobs (home screen, dialer, browser, keyboard...): never offered for removal.
  const [protections, setProtections] = useState<{ map: Map<string, string>; known: boolean }>({ map: new Map(), known: false });
  const [adbStatus, setAdbStatus] = useState<AdbStatus | null>(null);
  const [showAdbSetup, setShowAdbSetup] = useState(false);
  const refreshAdbStatus = useCallback(() => { ipc.adb.status().then(setAdbStatus).catch(() => {}); }, []);
  useEffect(() => { refreshAdbStatus(); }, [refreshAdbStatus]);
  const { plain, guideSeen } = useSettings();
  const [showGuide, setShowGuide] = useState(false);
  const closeGuide = () => { setShowGuide(false); updateSettings({ guideSeen: true }); };
  useEffect(() => { connectOpen.current = showConnect; }, [showConnect]);
  const [adbWarning, setAdbWarning] = useState("");
  const [labelProgress, setLabelProgress] = useState<LabelProgress | null>(null);
  const labelMap = useRef(new Map<string, { label: string; source: NameSource }>());
  const detailMap = useRef(new Map<string, AppInfo["details"]>());
  const [namesStatus, setNamesStatus] = useState<NamesStatusInfo | null>(null);
  const [namesRun, setNamesRun] = useState(0);
  const missing = useRef(0);

  /** Refresh the device list; `select` picks a just-connected device once it is ready. */
  const refreshDevices = useCallback(async (select?: string) => {
    try {
      const d = await ipc.listDevices();
      setDevices(d);
      setSerial((cur) => {
        const r = pickSerial(cur, d, select, missing.current);
        missing.current = r.misses;
        return r.serial;
      });
      setError("");
    } catch (e) { setError(String(e)); }
  }, []);

  /** After connecting wirelessly the device can take a moment to become ready. */
  const onConnectionChanged = useCallback((select?: string) => {
    refreshDevices(select);
    setTimeout(() => refreshDevices(select), 1500);
    setTimeout(() => refreshDevices(select), 3500);
  }, [refreshDevices]);

  const loadApps = useCallback(async (s: string) => {
    if (!s) { setApps([]); return; }
    setLoading(true);
    try {
      const list = await ipc.listApps(s);
      // Keep any real names that already arrived.
      setApps(list.map((a) => { const l = labelMap.current.get(a.name); return { ...a, ...(l ? { label: l.label, nameSource: l.source } : {}), details: detailMap.current.get(a.name) }; }));
      setSelected(new Set()); setError("");
    }
    catch (e) { setError(String(e)); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => { refreshDevices(); }, [refreshDevices]);

  // Action history survives restarts.
  useEffect(() => {
    ipc.history.load().then((h) => { if (Array.isArray(h)) setLog((cur) => (cur.length ? cur : (h as LogEntry[]))); }).catch(() => {}).finally(() => { historyReady.current = true; });
  }, []);
  useEffect(() => { if (historyReady.current) ipc.history.save(log).catch(() => {}); }, [log]);
  useEffect(() => { if (!notice) return; const t = setTimeout(() => setNotice(""), 7000); return () => clearTimeout(t); }, [notice]);
  useEffect(() => {
    ipc.wireless.check().then((c) => setAdbWarning(c.ok ? "" : c.message)).catch(() => {});
    // Notice phones being plugged in, unplugged or dropping off Wi-Fi.
    const t = setInterval(() => { if (!document.hidden && !connectOpen.current) refreshDevices(); }, 5000);
    return () => clearInterval(t);
  }, [refreshDevices]);
  useEffect(() => { loadApps(serial); }, [serial, loadApps]);
  useEffect(() => {
    setProtections({ map: new Map(), known: false });
    if (!serial) return;
    let cancelled = false;
    ipc.protections(serial)
      .then((list: Protection[]) => !cancelled && setProtections({ map: new Map(list.map((p) => [p.package, p.reason])), known: list.length > 0 }))
      .catch(() => {});
    return () => { cancelled = true; };
  }, [serial]);

  // Real names, details and icons stream in from the backend after the list is shown.
  useEffect(() => {
    labelMap.current = new Map();
    detailMap.current = new Map();
    clearIcons();
    setLabelProgress(null);
    setNamesStatus(null);
    if (!serial) return;
    let cancelled = false;
    let off = () => {};
    ipc.loadLabels(serial, {
      onLabels: (items) => {
        if (cancelled) return;
        for (const i of items) labelMap.current.set(i.name, { label: i.label, source: i.source ?? "file" });
        const m = new Map(items.map((i) => [i.name, i]));
        setApps((prev) => prev.map((a) => { const i = m.get(a.name); return i ? { ...a, label: i.label, nameSource: i.source ?? "file" } : a; }));
      },
      onProgress: (p) => !cancelled && setLabelProgress(p),
      onStatus: (st) => !cancelled && setNamesStatus(st),
      onDetails: (items) => {
        if (cancelled) return;
        for (const { name, ...d } of items) detailMap.current.set(name, d);
        const m = new Map(items.map((i) => [i.name, i]));
        setApps((prev) => prev.map((a) => { const d = m.get(a.name); if (!d) return a; const { name: _n, ...rest } = d; return { ...a, details: rest }; }));
      },
      onIcons: (items) => !cancelled && addIcons(items),
    }).then((f) => (cancelled ? f() : (off = f))).catch(() => {});
    return () => { cancelled = true; off(); };
  }, [serial, namesRun]);

  const byName = useMemo(() => new Map(apps.map((a) => [a.name, a])), [apps]);
  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = apps.filter((a) =>
      (scope === "all" || (scope === "system") === a.system) &&
      (stateFilter === "any" || a.state === stateFilter) &&
      (riskFilter === "any" || a.risk === riskFilter) &&
      matchesFilters(a, filters, byName) &&
      (!q || a.name.toLowerCase().includes(q) || a.label.toLowerCase().includes(q)));
    list.sort((a, b) => {
      const c = sort.key === "risk" ? riskOrder[a.risk] - riskOrder[b.risk] : a[sort.key].localeCompare(b[sort.key]);
      return c * sort.dir;
    });
    return list;
  }, [apps, query, scope, stateFilter, riskFilter, sort, filters, byName]);
  const categories = useMemo(() => categoryCounts(apps), [apps]);
  const haveInstallerData = useMemo(() => apps.some((a) => a.details), [apps]);

  const toggle = (name: string) => setSelected((s) => { const n = new Set(s); n.has(name) ? n.delete(name) : n.add(name); return n; });
  const allVisibleSelected = visible.length > 0 && visible.every((a) => selected.has(a.name));
  const toggleAll = () => setSelected(allVisibleSelected ? new Set() : new Set(visible.map((a) => a.name)));
  const sortBy = (key: typeof sort.key) => setSort((s) => ({ key, dir: s.key === key ? (-s.dir as 1 | -1) : 1 }));

  const selectedApps = apps.filter((a) => selected.has(a.name));
  const unsafeCount = (names: string[]) => apps.filter((a) => names.includes(a.name) && a.risk === "unsafe").length;

  const [alsoOn, setAlsoOn] = useState<Set<string>>(new Set());
  useEffect(() => { setAlsoOn(new Set()); }, [pending]);

  /** Repeat the same action on other phones the user ticked, each with its own safety snapshot. */
  async function runOthers(action: Action, names: string[]): Promise<string[]> {
    const notes: string[] = [];
    for (const s2 of alsoOn) {
      const label = devices.find((d) => d.serial === s2)?.model ?? s2;
      try {
        const theirs = await ipc.listApps(s2);
        const { names: todo, skipped } = appliesTo(action, names, theirs);
        if (!todo.length) { notes.push(`${label}: nothing to do (${skipped} not applicable)`); continue; }
        if (action === "disable" || action === "uninstall") await ipc.snapshots.create(s2, `Before ${action} of ${todo.length} app${todo.length > 1 ? "s" : ""}`, true);
        const results = await ipc.applyAction(s2, action, todo);
        const time = new Date().toLocaleTimeString();
        setLog((l) => [...results.map((r) => ({ time, serial: s2, action, name: r.name, ok: r.ok, message: r.message, prev: theirs.find((a) => a.name === r.name)?.state ?? "enabled" })), ...l]);
        const bad = results.filter((r) => !r.ok).length;
        notes.push(`${label}: ${results.length - bad} done${bad ? `, ${bad} failed` : ""}${skipped ? `, ${skipped} not applicable` : ""}`);
      } catch (e) { notes.push(`${label}: not changed (${String(e)})`); }
    }
    return notes;
  }

  async function run(action: Action, names: string[]) {
    setBusy(true);
    if ((action === "disable" || action === "uninstall") && names.length > 0) {
      // Safety net: save the current state first. If that fails nothing is changed.
      try {
        const snap = await ipc.snapshots.create(serial, `Before ${action} of ${names.length} app${names.length > 1 ? "s" : ""}`, true);
        setSnapVersion((v) => v + 1);
        setNotice(`Safety snapshot saved: "${snap.name}". Undo from the Snapshots tab.`);
      } catch (e) {
        setError(`Could not save a safety snapshot, so nothing was changed. ${String(e)}`);
        setBusy(false); setPending(null);
        return;
      }
    }
    const prevs = new Map(apps.map((a) => [a.name, a.state]));
    try {
      const results = await ipc.applyAction(serial, action, names);
      const time = new Date().toLocaleTimeString();
      setLog((l) => [...results.map((r) => ({ time, serial, action, name: r.name, ok: r.ok, message: r.message, prev: prevs.get(r.name) ?? "enabled" })), ...l]);
      await loadApps(serial);
      if (alsoOn.size > 0 && repeatable(action)) setNotice(`Also on other phones: ${(await runOthers(action, names)).join("; ")}`);
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); setPending(null); }
  }

  const eligible = (action: Action) => selectedApps.filter((a) =>
    action === "disable" ? a.state === "enabled" : action === "enable" ? a.state === "disabled" :
    action === "uninstall" ? a.state !== "uninstalled" : a.state === "uninstalled").map((a) => a.name);

  const nameFor = (pkg: string) => apps.find((a) => a.name === pkg)?.label ?? pkg;
  const device = devices.find((d) => d.serial === serial);
  const counts = { total: apps.length, disabled: apps.filter((a) => a.state === "disabled").length, removed: apps.filter((a) => a.state === "uninstalled").length };

  return (
    <div className="app">
      <header>
        <h1 className="brand"><img src="/logo.svg" width="26" height="26" alt="" />{BRAND.name}</h1>
        {ipc.isMock && <span className="badge mock" title="Running in a browser without the Rust backend">mock data</span>}
        <nav className="tabs">
          <button className={tab === "apps" ? "active" : ""} onClick={() => setTab("apps")}>{t("tab.apps")}</button>
          <button className={tab === "running" ? "active" : ""} onClick={() => setTab("running")}>{t("tab.running")}</button>
          <button className={tab === "health" ? "active" : ""} onClick={() => setTab("health")}>{t("tab.health")}</button>
          <button className={tab === "debloat" ? "active" : ""} onClick={() => setTab("debloat")}>{t("tab.debloat")}</button>
          <button className={tab === "tune" ? "active" : ""} onClick={() => setTab("tune")}>{t("tab.tune")}</button>
          <button className={tab === "install" ? "active" : ""} onClick={() => setTab("install")}>{t("tab.install")}</button>
          <button className={tab === "storage" ? "active" : ""} onClick={() => setTab("storage")}>{t("tab.storage")}</button>
          <button className={tab === "ratings" ? "active" : ""} onClick={() => setTab("ratings")}>{t("tab.ratings")}</button>
          <button className={tab === "phones" ? "active" : ""} onClick={() => setTab("phones")}>{t("tab.phones")}{devices.length > 1 ? ` (${devices.length})` : ""}</button>
          <button className={tab === "wake" ? "active" : ""} onClick={() => setTab("wake")}>{t("tab.wake")}</button>
          <button className={tab === "snapshots" ? "active" : ""} onClick={() => setTab("snapshots")}>{t("tab.snapshots")}</button>
          <button className={tab === "logs" ? "active" : ""} onClick={() => setTab("logs")}>{t("tab.logs")}</button>
        </nav>
        <div className="spacer" />
        <select value={serial} onChange={(e) => setSerial(e.target.value)} aria-label="Device">
          <option value="">{devices.length ? t("header.selectDevice") : t("header.noDevices")}</option>
          {devices.map((d) => (
            <option key={d.serial} value={d.serial} disabled={d.state !== "device"}>
              {isWireless(d.serial) ? "Wi-Fi" : "USB"} · {d.model ?? d.serial} — {d.state === "device" ? d.serial : d.state}
            </option>
          ))}
        </select>
        <button onClick={() => refreshDevices()}>{t("header.refresh")}</button>
        {serial && isWireless(serial) && (
          <button onClick={async () => { try { await ipc.wireless.disconnect(serial); } catch (e) { setError(String(e)); } refreshDevices(); }}>{t("header.disconnect")}</button>
        )}
        <button className="primary" onClick={() => setShowConnect(true)}>{t("header.connect")}</button>
        <button onClick={() => setShowGuide(true)} aria-label="Help getting started" title={t("header.help")}>?</button>
        <button onClick={() => setShowSettings(true)} aria-label={t("header.settings")} title={t("header.settings")}>⚙</button>
      </header>

      {error && <div className="error" role="alert">{error}</div>}
      {adbWarning && adbStatus?.found !== false && <div className="error" role="alert">{adbWarning}</div>}
      {adbStatus && !adbStatus.found && (
        <div className="error" role="alert">
          adb, the tool that talks to your phone, was not found. <button className="linklike" onClick={() => setShowAdbSetup(true)}>Set it up now</button>
        </div>
      )}
      {notice && <div className="notice" role="status">{notice} <button className="linklike" onClick={() => setTab("snapshots")}>Open Snapshots</button></div>}
      <DeviceCard serial={serial} />
      {device?.state === "unauthorized" && <div className="error">Accept the USB debugging prompt on the phone.</div>}

      {tab === "running" && (
        <Running serial={serial} wireless={isWireless(serial)} apps={apps} active onRequest={(action, names) => setPending({ action, names })} />
      )}

      {tab === "health" && <Health serial={serial} wireless={isWireless(serial)} active onShowRunning={() => setTab("running")} onShowTune={() => setTab("tune")} />}

      {tab === "debloat" && <Debloat serial={serial} apps={apps} busy={busy} protections={protections.map} protectionsKnown={protections.known} onApply={(action, names) => setPending({ action, names })} />}

      {tab === "storage" && <Storage serial={serial} apps={apps} protections={protections.map} busy={busy} active onRequest={(action, names) => setPending({ action, names })} />}

      {tab === "ratings" && <Ratings apps={apps} onChanged={() => loadApps(serial)} />}

      {tab === "phones" && <Phones devices={devices} serial={serial} active onUse={(s) => { setSerial(s); setTab("apps"); }} />}

      {tab === "wake" && <Wake serial={serial} apps={apps} active />}

      {tab === "install" && <InstallAndBackup serial={serial} deviceLabel={devices.find((d) => d.serial === serial)?.model ?? serial} apps={apps} active onInstalled={() => loadApps(serial)} />}

      {tab === "logs" && <Logs active />}

      {tab === "tune" && <Tune serial={serial} apps={apps} active />}

      {tab === "snapshots" && <Snapshots serial={serial} version={snapVersion} onRestored={() => loadApps(serial)} />}

      {tab === "apps" && <>
      <div className="toolbar">
        <input type="search" placeholder={t("apps.search")} value={query} onChange={(e) => setQuery(e.target.value)} />
        <select value={scope} onChange={(e) => setScope(e.target.value as Scope)} aria-label="Type">
          <option value="all">{t("apps.allApps")}</option><option value="system">{t("apps.system")}</option><option value="user">{t("apps.user")}</option>
        </select>
        <select value={stateFilter} onChange={(e) => setStateFilter(e.target.value as StateFilter)} aria-label="State">
          <option value="any">{t("apps.anyState")}</option><option value="enabled">{t("apps.enabled")}</option><option value="disabled">{t("apps.disabled")}</option><option value="uninstalled">{t("apps.uninstalled")}</option>
        </select>
        <select value={riskFilter} onChange={(e) => setRiskFilter(e.target.value as Risk | "any")} aria-label="Risk">
          <option value="any">{t("apps.anyRisk")}</option><option value="recommended">{t("apps.recommended")}</option><option value="advanced">{t("apps.advanced")}</option><option value="unsafe">{t("apps.unsafe")}</option><option value="unknown">{t("apps.unrated")}</option>
        </select>
        <select value={filters.category} onChange={(e) => setFilters({ ...filters, category: e.target.value })} aria-label="Category">
          <option value="any">Any kind of app</option>
          {categories.map((c) => <option key={c.key} value={c.key}>{nameOfCategory(c.key)} ({c.count})</option>)}
        </select>
        <select value={filters.installer} onChange={(e) => setFilters({ ...filters, installer: e.target.value as InstallerKey })} aria-label="Installed by" disabled={!haveInstallerData}
          title={haveInstallerData ? undefined : "Available once the phone has told us who installed each app"}>
          {INSTALLER_OPTIONS.map((o) => <option key={o.key} value={o.key}>{o.label}</option>)}
        </select>
        <label className="inline" title="Apps that do the same job as another app you also have"><input type="checkbox" checked={filters.duplicatesOnly} onChange={(e) => setFilters({ ...filters, duplicatesOnly: e.target.checked })} /> Duplicates only</label>
        <button onClick={() => loadApps(serial)} disabled={!serial || loading}>{loading ? t("common.loading") : t("common.reload")}</button>
      </div>

      <div className="actions">
        <span>{selected.size} selected · {visible.length} shown · {counts.total} total · {counts.disabled} disabled · {counts.removed} uninstalled</span>
        <NamesStatus status={namesStatus} progress={labelProgress} onRetry={() => setNamesRun((n) => n + 1)} nameOf={(p) => apps.find((a) => a.name === p)?.label ?? p} />
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
                {!plain && (
                <th onClick={() => sortBy("name")}>Package {sort.key === "name" && (sort.dir === 1 ? "▲" : "▼")}</th>
                )}
                <th>Type</th><th>State</th>
                <th onClick={() => sortBy("risk")}>Rating {sort.key === "risk" && (sort.dir === 1 ? "▲" : "▼")}</th>
                <th title="Is it safe to disable?">Disable</th><th title="Is it safe to uninstall?">Uninstall</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((a) => (
                <tr key={a.name} className={selected.has(a.name) ? "sel" : ""} onClick={() => toggle(a.name)} title={[a.description, a.details?.versionName && `Version ${a.details.versionName}`, a.details?.installer && `Installed by ${a.details.installer}`, a.details && (a.details.launchable ? "Has a launcher icon" : "No launcher icon")].filter(Boolean).join(" · ") || undefined}>
                  <td className="chk"><input type="checkbox" checked={selected.has(a.name)} onChange={() => toggle(a.name)} onClick={(e) => e.stopPropagation()} /></td>
                  <td><span className="namecell"><AppIcon name={a.name} /><span><span className={a.nameSource ? "" : "guess"} title={a.nameSource ? undefined : "Not confirmed: guessed from the package name"}>{a.label}</span>{plain && <span className="reason">{reasonOf(a)}</span>}{duplicateTarget(a, byName) && <span className="reason">Does the same job as {duplicateTarget(a, byName)!.label}, which you also have.</span>}{protections.map.get(a.name) && <span className="reason warn-inline">You use this as {protections.map.get(a.name)}.</span>}</span></span></td>
                  {!plain && <td className="mono">{a.name}</td>}
                  <td>{a.system ? "System" : "User"}</td>
                  <td><span className={`pill ${a.state}`}>{a.state}</span></td>
                  <td><span className={`pill risk-${a.risk}`}>{a.risk === "unknown" ? "unrated" : a.risk}</span></td>
                  <td><SafetyTag kind="Disable" value={a.disableSafety} /></td>
                  <td><SafetyTag kind="Uninstall" value={a.uninstallSafety} /></td>
                </tr>
              ))}
              {!visible.length && <tr><td colSpan={plain ? 7 : 8} className="empty">{serial ? (loading ? "Loading apps…" : "No apps match.") : "Connect a device with USB debugging enabled and select it above."}</td></tr>}
            </tbody>
          </table>
        </div>

        {showLog && (
          <aside className="log">
            <h2>History</h2>
            {!log.length && <p className="muted">Nothing yet.</p>}
            {log.map((e, i) => (
              <div key={i} className={`entry ${e.ok ? "" : "fail"}`}>
                <div><b>{labelOf[e.action]}</b> <span className={plain ? "" : "mono"}>{plain ? nameFor(e.name) : e.name}</span></div>
                <div className="muted">{e.time} · {e.ok ? "ok" : e.message ?? "failed"}</div>
                {e.ok && undoOf[e.action] && <button disabled={busy || e.serial !== serial} title={e.serial !== serial ? "Select the device this was done on to undo" : undefined} onClick={() => run(undoOf[e.action]!, [e.name])}>Undo</button>}
              </div>
            ))}
          </aside>
        )}
      </div>
      </>}

      {(showGuide || !guideSeen) && (
        <Guide adbStatus={adbStatus} devices={devices} serial={serial} onAdbInstalled={() => { refreshAdbStatus(); refreshDevices(); }}
          onOpenConnect={() => setShowConnect(true)} onClose={closeGuide} />
      )}
      {showAdbSetup && (
        <div className="modal-bg" onClick={() => setShowAdbSetup(false)}>
          <div className="modal wide" role="dialog" aria-label="Set up adb" onClick={(e) => e.stopPropagation()}>
            <h2>Set up adb</h2>
            <AdbSetup status={adbStatus} onInstalled={() => { refreshAdbStatus(); refreshDevices(); }} />
            <div className="modal-actions"><button onClick={() => setShowAdbSetup(false)}>Close</button></div>
          </div>
        </div>
      )}
      {showSettings && <SettingsDialog onClose={() => setShowSettings(false)} />}
      {showConnect && <ConnectDialog devices={devices} onClose={() => setShowConnect(false)} onChanged={onConnectionChanged} />}

      {pending && (
        <div className="modal-bg" onClick={() => !busy && setPending(null)}>
          <div className="modal" role="dialog" onClick={(e) => e.stopPropagation()}>
            <h2>{t(pending.names.length > 1 ? "confirm.many" : "confirm.one", { action: labelOf[pending.action], n: pending.names.length })}</h2>
            {unsafeCount(pending.names) > 0 && (pending.action === "disable" || pending.action === "uninstall") && (
              <p className="warn">⚠ {unsafeCount(pending.names)} selected app(s) are rated <b>unsafe</b>. Removing core system packages can cause a bootloop.</p>
            )}
            <ul className="pendinglist">
              {pending.names.slice(0, 8).map((n) => {
                const a = apps.find((x) => x.name === n);
                const used = protections.map.get(n);
                const removing = pending.action === "disable" || pending.action === "uninstall";
                return (
                  <li key={n} className={plain ? "" : "mono"}>
                    {plain ? nameFor(n) : n}
                    {removing && used && <div className="warn-inline">⚠ This is {used}. Make sure another one is set up first.</div>}
                    {removing && a?.impact && <div className="reason">{a.impact}</div>}
                    {removing && a && a.tier === "unrated" && <div className="reason">Nobody has rated this app yet. Only continue if you know what it is.</div>}
                  </li>
                );
              })}
              {pending.names.length > 8 && <li>…and {pending.names.length - 8} more</li>}
            </ul>
            {repeatable(pending.action) && otherReady(devices, serial).length > 0 && (
              <fieldset className="alsoon">
                <legend>{t("confirm.alsoOn")}</legend>
                {otherReady(devices, serial).map((d) => (
                  <label key={d.serial} className="inline"><input type="checkbox" checked={alsoOn.has(d.serial)} disabled={busy} onChange={(e) => setAlsoOn((s) => { const n = new Set(s); e.target.checked ? n.add(d.serial) : n.delete(d.serial); return n; })} /> {d.model ?? d.serial}</label>
                ))}
                <div className="muted small">Only apps that phone also has are changed; each phone saves its own safety snapshot first.</div>
              </fieldset>
            )}
            <p className="muted">{pending.action === "uninstall" ? "Removed for the current user only; can be restored later." : pending.action === "stop" ? "Stops the app now; it may restart on its own." : "Reversible from the History panel."}</p>
            <div className="modal-actions">
              <button onClick={() => setPending(null)} disabled={busy}>{t("common.cancel")}</button>
              <button className={pending.action === "uninstall" ? "danger" : "primary"} disabled={busy} onClick={() => run(pending.action, pending.names)}>{busy ? t("common.working") : labelOf[pending.action]}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
