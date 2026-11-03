import { useEffect, useMemo, useRef, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { AppInfo, ExportResult, InstallItem, InstallOptions } from "../../lib/types";
import { AppIcon } from "../../components/AppIcon";
import { useSettings } from "../../lib/settings";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);
const size = (b: number) => (b >= 1_048_576 ? `${(b / 1_048_576).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);
const KIND: Record<string, string> = { apk: "app file", splits: "app in several parts", apks: "app bundle", xapk: "app bundle (XAPK)", apkm: "app bundle (APKM)", folder: "folder of app files" };

interface Entry { key: string; item?: InstallItem; error?: string; status: "ready" | "installing" | "done" | "failed"; message?: string }

interface Props {
  serial: string;
  deviceLabel: string;
  apps: AppInfo[];
  active: boolean;
  /** Called after something was installed so the app list can reload. */
  onInstalled: () => void;
}

export function InstallAndBackup({ serial, deviceLabel, apps, active, onInstalled }: Props) {
  return (
    <div className="installtab">
      <InstallPanel serial={serial} deviceLabel={deviceLabel} active={active} onInstalled={onInstalled} />
      <BackupPanel serial={serial} apps={apps} />
    </div>
  );
}

// ---------------------------------------------------------------- install

function InstallPanel({ serial, deviceLabel, active, onInstalled }: Pick<Props, "serial" | "deviceLabel" | "active" | "onInstalled">) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [hover, setHover] = useState(false);
  const [opts, setOpts] = useState<InstallOptions>({ replace: true, allowDowngrade: false, allowTest: false });
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const entriesRef = useRef(entries);
  entriesRef.current = entries;

  const add = async (paths: string[]) => {
    if (!paths.length) return;
    setNote("Looking at the files…");
    try {
      const res = await ipc.install.inspect(paths);
      setEntries((prev) => [...prev, ...res.map((r, i): Entry => ({ key: `${Date.now()}-${i}-${Math.random()}`, item: r.item ?? undefined, error: r.error ?? undefined, status: r.item ? "ready" : "failed", message: r.error ?? undefined }))]);
      setNote("");
    } catch (e) { setNote(errText(e)); }
  };

  // Files dropped onto the window while this tab is open.
  useEffect(() => {
    if (!active) return;
    let off = () => {}; let gone = false;
    ipc.install.onDrop((p) => add(p), setHover).then((f) => (gone ? f() : (off = f))).catch(() => {});
    return () => { gone = true; off(); setHover(false); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);
  // Clean up unpacked bundles when leaving.
  useEffect(() => () => { const ids = entriesRef.current.flatMap((e) => (e.item ? [e.item.id] : [])); if (ids.length) ipc.install.discard(ids).catch(() => {}); }, []);

  const ready = entries.filter((e) => e.status === "ready" && e.item);
  const remove = (e: Entry) => { if (e.item) ipc.install.discard([e.item.id]).catch(() => {}); setEntries((p) => p.filter((x) => x.key !== e.key)); };

  const run = async () => {
    setConfirm(false); setBusy(true);
    let installed = 0;
    for (const e of ready) {
      setEntries((p) => p.map((x) => (x.key === e.key ? { ...x, status: "installing", message: undefined } : x)));
      try {
        const msg = await ipc.install.run(serial, e.item!.id, opts);
        installed++;
        setEntries((p) => p.map((x) => (x.key === e.key ? { ...x, status: "done", message: msg } : x)));
      } catch (err) {
        setEntries((p) => p.map((x) => (x.key === e.key ? { ...x, status: "failed", message: errText(err) } : x)));
      }
    }
    setBusy(false);
    if (installed) onInstalled();
  };

  return (
    <section className="block">
      <h3>Install apps from files</h3>
      <div className={`dropzone ${hover ? "hover" : ""}`} aria-label="Drop apps here">
        <p><b>Drop app files here</b> (.apk, .apks, .xapk, .apkm) or a folder of .apk files</p>
        <div className="row" style={{ justifyContent: "center" }}>
          <button onClick={async () => add(await ipc.install.pick("files"))}>Choose files…</button>
          <button onClick={async () => add(await ipc.install.pick("folder"))}>Choose a folder…</button>
        </div>
        <p className="muted small">Apps made of several parts are installed together. Only install apps from sources you trust.</p>
      </div>
      {note && <p className="muted">{note}</p>}
      {entries.length > 0 && (
        <table className="installtable">
          <tbody>
            {entries.map((e) => (
              <tr key={e.key} className="static">
                <td>
                  <div><b>{e.item?.label ?? e.item?.source ?? "Not installable"}</b> {e.item && <span className="muted small">{KIND[e.item.kind] ?? e.item.kind} · {e.item.fileCount} file{e.item.fileCount === 1 ? "" : "s"} · {size(e.item.sizeBytes)}{e.item.version ? ` · version ${e.item.version}` : ""}</span>}</div>
                  {e.item?.warnings.map((w) => <div key={w} className="warn-inline small">⚠ {w}</div>)}
                  {e.message && <div className={e.status === "done" ? "ok-mark small" : "bad-mark small"} role={e.status === "failed" ? "alert" : "status"}>{e.status === "done" ? "✓ " : "✗ "}{e.message}</div>}
                </td>
                <td className="num">{e.status === "installing" ? <span className="muted">installing…</span> : e.status === "ready" ? <span className="muted">ready</span> : null}</td>
                <td className="rowbtns"><button disabled={busy && e.status === "installing"} onClick={() => remove(e)}>Remove</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="row" style={{ marginTop: 10 }}>
        <label className="inline"><input type="checkbox" checked={opts.replace} onChange={(e) => setOpts({ ...opts, replace: e.target.checked })} /> Replace if already installed</label>
        <label className="inline" title="Not every phone allows this"><input type="checkbox" checked={opts.allowDowngrade} onChange={(e) => setOpts({ ...opts, allowDowngrade: e.target.checked })} /> Allow an older version</label>
        <label className="inline"><input type="checkbox" checked={opts.allowTest} onChange={(e) => setOpts({ ...opts, allowTest: e.target.checked })} /> Allow test builds</label>
        <div className="spacer" />
        <button className="primary" disabled={!serial || busy || ready.length === 0} onClick={() => setConfirm(true)}>Install {ready.length || ""} on {deviceLabel || "the phone"}…</button>
      </div>
      {confirm && (
        <div className="modal-bg" onClick={() => setConfirm(false)}>
          <div className="modal" role="dialog" aria-label="Confirm install" onClick={(e) => e.stopPropagation()}>
            <h2>Install {ready.length} app{ready.length === 1 ? "" : "s"} on {deviceLabel || "the phone"}?</h2>
            <ul className="pendinglist">
              {ready.map((e) => <li key={e.key}><b>{e.item!.label ?? e.item!.source}</b>{e.item!.package && <span className="muted small"> {e.item!.package}</span>}<div className="reason">{e.item!.version ? `Version ${e.item!.version}. ` : ""}Asks for {e.item!.permissions} permission{e.item!.permissions === 1 ? "" : "s"} when installed.</div></li>)}
            </ul>
            <p className="muted">Only install apps from sources you trust. An app you replace keeps its data.</p>
            <div className="modal-actions"><button onClick={() => setConfirm(false)}>Cancel</button><button className="primary" onClick={run}>Install</button></div>
          </div>
        </div>
      )}
    </section>
  );
}

// ---------------------------------------------------------------- backup

interface Result { pkg: string; ok: boolean; text: string }

function BackupPanel({ serial, apps }: { serial: string; apps: AppInfo[] }) {
  const { plain } = useSettings();
  const [includeSystem, setIncludeSystem] = useState(false);
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [results, setResults] = useState<Result[]>([]);
  const [folder, setFolder] = useState("");

  const list = useMemo(() => {
    const q = query.trim().toLowerCase();
    return apps.filter((a) => a.state !== "uninstalled" && (includeSystem || !a.system) && (!q || a.label.toLowerCase().includes(q) || a.name.toLowerCase().includes(q)))
      .sort((a, b) => a.label.localeCompare(b.label));
  }, [apps, includeSystem, query]);
  const allOn = list.length > 0 && list.every((a) => picked.has(a.name));
  const toggle = (n: string) => setPicked((s) => { const x = new Set(s); x.has(n) ? x.delete(n) : x.add(n); return x; });

  const start = async () => {
    const dest = await ipc.backup.chooseFolder();
    if (!dest) return;
    const names = [...picked];
    setBusy(true); setResults([]); setFolder(dest); setProgress({ done: 0, total: names.length });
    for (let i = 0; i < names.length; i++) {
      const a = apps.find((x) => x.name === names[i]);
      try {
        const r: ExportResult = await ipc.backup.one(serial, names[i], dest, { label: a?.label, versionName: a?.details?.versionName, installer: a?.details?.installer });
        setResults((p) => [...p, { pkg: names[i], ok: true, text: `${r.files.length} file${r.files.length === 1 ? "" : "s"}, ${size(r.bytes)}` }]);
      } catch (e) { setResults((p) => [...p, { pkg: names[i], ok: false, text: errText(e) }]); }
      setProgress({ done: i + 1, total: names.length });
    }
    setBusy(false);
  };

  return (
    <section className="block">
      <h3>Back up apps to your computer</h3>
      <p className="muted small">Saves each app's files (every part of apps made of several parts) in its own folder, with a small info file. Only the app is saved, not its data. To put one back, drop its folder in the box above.</p>
      <div className="row">
        <input type="search" placeholder="Search apps…" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search apps to back up" />
        <label className="inline"><input type="checkbox" checked={includeSystem} onChange={(e) => setIncludeSystem(e.target.checked)} /> Include apps that came with the phone</label>
        <div className="spacer" />
        <button onClick={() => setPicked(allOn ? new Set() : new Set(list.map((a) => a.name)))} disabled={!list.length}>{allOn ? "Untick all" : "Tick all shown"}</button>
        <button className="primary" disabled={!serial || busy || picked.size === 0} onClick={start}>Back up {picked.size || ""} app{picked.size === 1 ? "" : "s"}…</button>
      </div>
      <div className="backuplist" role="group" aria-label="Apps to back up">
        {list.map((a) => (
          <label key={a.name} className="backuprow">
            <input type="checkbox" checked={picked.has(a.name)} onChange={() => toggle(a.name)} aria-label={`Back up ${a.label}`} />
            <AppIcon name={a.name} size={20} /> <span>{a.label}</span>{!plain && <span className="mono muted"> {a.name}</span>}
          </label>
        ))}
        {!list.length && <p className="muted">{serial ? "No apps match." : "Connect a phone to see its apps."}</p>}
      </div>
      {progress && (
        <div className="beforeafter" role="status">
          {busy ? `Backing up… ${progress.done} of ${progress.total}` : `Done: ${results.filter((r) => r.ok).length} saved, ${results.filter((r) => !r.ok).length} could not be saved. Folder: ${folder}`}
        </div>
      )}
      {results.length > 0 && (
        <ul className="steps-list" aria-label="Backup results">
          {results.map((r) => <li key={r.pkg} className={r.ok ? "ok" : "bad"}>{r.ok ? "✓" : "✗"} {apps.find((a) => a.name === r.pkg)?.label ?? r.pkg} <span className="muted">{r.text}</span></li>)}
        </ul>
      )}
    </section>
  );
}
