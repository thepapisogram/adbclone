import { useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { AdbStatus, Device } from "../../lib/types";
import { AdbSetup } from "../setup/AdbSetup";
import { MAKERS, makerFor, type MakerKey } from "./makers";

interface Props {
  adbStatus: AdbStatus | null;
  devices: Device[];
  serial: string;
  onAdbInstalled: () => void;
  onOpenConnect: () => void;
  onClose: () => void;
}

const Tick = ({ ok, warn }: { ok: boolean; warn?: boolean }) => <span className={ok ? "ok-mark" : warn ? "warn-inline" : "muted"} aria-hidden="true">{ok ? "✓" : warn ? "!" : "○"}</span>;

/** Step-by-step help to get a phone connected, with live checks and notes for the phone's maker. */
export function Guide({ adbStatus, devices, serial, onAdbInstalled, onOpenConnect, onClose }: Props) {
  const [maker, setMaker] = useState<MakerKey>("other");
  const [auto, setAuto] = useState(true);

  // Pick the maker automatically when a phone is connected, until the person chooses one themselves.
  useEffect(() => {
    if (!serial || !auto) return;
    let cancelled = false;
    ipc.deviceInfo(serial).then((i) => !cancelled && setMaker(makerFor(i.manufacturer))).catch(() => {});
    return () => { cancelled = true; };
  }, [serial, auto]);

  const ready = devices.filter((d) => d.state === "device");
  const unauthorized = devices.filter((d) => d.state === "unauthorized");
  const m = MAKERS.find((x) => x.key === maker)!;

  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal wide guide" role="dialog" aria-label="Getting started" onClick={(e) => e.stopPropagation()}>
        <h2>Getting started</h2>
        <ul className="checks" aria-label="Status">
          <li><Tick ok={!!adbStatus?.found} /> adb (the tool that talks to your phone) is {adbStatus?.found ? "ready" : "not set up yet"}</li>
          <li><Tick ok={ready.length > 0} /> A phone is {ready.length > 0 ? `connected (${ready.length})` : "not connected yet"}</li>
          {unauthorized.length > 0 && <li><Tick ok={false} warn /> A phone is connected but has not allowed this computer: look at the phone's screen and tap Allow.</li>}
        </ul>

        <h3>1. Set up adb</h3>
        <AdbSetup status={adbStatus} onInstalled={onAdbInstalled} />

        <h3>2. Turn on USB debugging on your phone</h3>
        <label className="inline">My phone is made by{" "}
          <select value={maker} onChange={(e) => { setAuto(false); setMaker(e.target.value as MakerKey); }} aria-label="Phone maker">
            {MAKERS.map((x) => <option key={x.key} value={x.key}>{x.name}</option>)}
          </select>
        </label>
        <ol className="steps">
          <li>Open <b>{m.buildNumber}</b> and tap <b>Build number</b> seven times. The phone says you are now a developer.</li>
          <li>Open <b>Settings &gt; System &gt; Developer options</b> (it may be under Additional settings) and turn on <b>USB debugging</b>.</li>
        </ol>
        <div className="makernote" aria-label={`${m.name} notes`}>
          <b>{m.name}</b>
          <ul>{m.notes.map((n) => <li key={n}>{n}</li>)}</ul>
        </div>

        <h3>3. Plug in and allow</h3>
        <ol className="steps">
          <li>Connect the cable. If the phone only charges, pull down the notification and choose <b>File transfer</b>.</li>
          <li>A box asks <b>Allow USB debugging?</b> Tick "Always allow from this computer" and tap Allow.</li>
          <li>Nothing appears? In Developer options choose <b>Revoke USB debugging authorizations</b>, unplug, and plug in again.</li>
        </ol>

        <h3>4. Optional: no cable</h3>
        <p className="muted small">
          Android 11 or newer: in Developer options turn on <b>Wireless debugging</b> (phone and computer on the same Wi-Fi), then use <b>Connect…</b> in the top bar.
          Older phones: connect with the cable once, then use "From USB" in the same window to switch to Wi-Fi.
        </p>
        <div className="row"><button onClick={() => { onClose(); onOpenConnect(); }}>Open Connect…</button></div>
        <p className="muted small">Menu names change between phones and Android versions. If yours differs, search Settings for "Build number" and "USB debugging".</p>

        <div className="modal-actions"><button className="primary" onClick={onClose}>Done</button></div>
      </div>
    </div>
  );
}
