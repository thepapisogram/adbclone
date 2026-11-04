import { useCallback, useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { Device, DeviceInfo, HealthSample } from "../../lib/types";

const errText = (e: unknown) => String(e instanceof Error ? e.message : e);
const gb = (kb?: number | null) => (kb == null ? "—" : `${(kb / 1_048_576).toFixed(0)} GB`);

interface Card { info?: DeviceInfo; health?: HealthSample; error?: string }
interface Props { devices: Device[]; serial: string; active: boolean; onUse: (serial: string) => void }

export function Phones({ devices, serial, active, onUse }: Props) {
  const [cards, setCards] = useState<Record<string, Card>>({});
  const [msg, setMsg] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const [busy, setBusy] = useState("");
  const [mirrorOk, setMirrorOk] = useState<boolean | null>(null);

  const ready = devices.filter((d) => d.state === "device");
  const key = ready.map((d) => d.serial).join(",");
  const load = useCallback(async () => {
    const next: Record<string, Card> = {};
    await Promise.all(ready.map(async (d) => {
      const [info, health] = await Promise.all([ipc.deviceInfo(d.serial).catch((e) => e), ipc.healthSample(d.serial).catch(() => undefined)]);
      next[d.serial] = info instanceof Error || typeof info === "string" ? { error: errText(info), health } : { info, health };
    }));
    setCards(next);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  useEffect(() => { if (active) { load(); ipc.mirrorAvailable().then(setMirrorOk).catch(() => setMirrorOk(false)); } }, [active, load]);

  const act = async (id: string, fn: () => Promise<string | null>) => {
    setBusy(id); setMsg(null);
    try { const t = await fn(); if (t) setMsg({ kind: "ok", text: t }); } catch (e) { setMsg({ kind: "error", text: errText(e) }); } finally { setBusy(""); }
  };

  if (!devices.length) return <div className="empty-state">No phone is connected. Plug one in, or use Connect for Wi-Fi.</div>;
  return (
    <div className="storage">
      {msg && <div className={`status ${msg.kind}`} role={msg.kind === "error" ? "alert" : "status"}>{msg.text}</div>}
      <p className="muted small">"Use this phone" switches the other tabs to it. When you disable or remove apps, you can tick "also do this on" other phones in the confirmation window.</p>
      <div className="presetgrid phonegrid" aria-label="Connected phones">
        {devices.map((d) => {
          const c = cards[d.serial];
          const isReady = d.state === "device";
          return (
            <div key={d.serial} className={`presetcard phonecard ${d.serial === serial ? "current" : ""}`} aria-label={`Phone ${d.model ?? d.serial}`}>
              <b>{c?.info?.manufacturer ? `${c.info.manufacturer} ${c.info.model ?? ""}` : d.model ?? d.serial}{d.serial === serial && <span className="pill"> in use</span>}</b>
              <span className="muted small mono">{d.serial.includes(":") ? "Wi-Fi · " : "USB · "}{d.serial}</span>
              {!isReady && <span className="status error">{d.state === "unauthorized" ? "Waiting for you to allow this computer on the phone." : `State: ${d.state}`}</span>}
              {isReady && !c && <span className="muted small">Reading…</span>}
              {c?.error && <span className="muted small">{c.error}</span>}
              {c?.info && <span className="small">Android {c.info.androidRelease ?? "?"} · storage free {gb(c.info.storageFreeKb)} of {gb(c.info.storageTotalKb)}</span>}
              {c?.health?.battery && <span className="small">Battery {c.health.battery.level ?? "—"}%{c.health.battery.tempC != null && ` · ${c.health.battery.tempC.toFixed(1)}°C`}{c.health.battery.charging && " · charging"}</span>}
              <div className="row">
                <button disabled={!isReady || d.serial === serial} onClick={() => onUse(d.serial)}>Use this phone</button>
                <button disabled={!isReady || !!busy} onClick={() => act(`shot-${d.serial}`, async () => { const p = await ipc.screenshot(d.serial); return p ? `Screenshot saved: ${p}` : null; })}>Screenshot…</button>
                <button disabled={!isReady || !!busy || mirrorOk === false} title={mirrorOk === false ? "scrcpy is not installed" : undefined} onClick={() => act(`mirror-${d.serial}`, async () => { await ipc.mirror(d.serial); return "Opening the mirror window…"; })}>Mirror screen</button>
              </div>
            </div>
          );
        })}
      </div>
      {mirrorOk === false && <p className="muted small">Mirroring uses scrcpy, a free open-source program that is not part of Coolant. Install it from github.com/Genymobile/scrcpy (put it on your PATH or next to Coolant) and the button will work.</p>}
    </div>
  );
}
