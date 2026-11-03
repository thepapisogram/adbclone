import { useCallback, useEffect, useRef, useState } from "react";
import { ipc } from "../../lib/ipc";
import { isWireless } from "../../lib/types";
import type { Device, MdnsService, QrStart, SavedDevice } from "../../lib/types";

type Tab = "pair" | "qr" | "usb" | "saved";
type Status = { kind: "ok" | "error" | "busy"; text: string } | null;

const ipOf = (a: string) => a.slice(0, a.lastIndexOf(":"));
const errText = (e: unknown) => String(e instanceof Error ? e.message : e);

function StatusLine({ s }: { s: Status }) {
  if (!s) return null;
  return <div className={`status ${s.kind}`} role={s.kind === "error" ? "alert" : "status"}>{s.text}</div>;
}

interface Props {
  devices: Device[];
  onClose: () => void;
  /** Called after any connection change so the device list can refresh. `serial` is set when a device should be selected. */
  onChanged: (serial?: string) => void;
}

export function ConnectDialog({ devices, onClose, onChanged }: Props) {
  const [tab, setTab] = useState<Tab>("pair");
  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal wide" role="dialog" aria-label="Connect device" onClick={(e) => e.stopPropagation()}>
        <h2>Connect a phone over Wi-Fi</h2>
        <nav className="tabs inner">
          {([["pair", "Pairing code"], ["qr", "QR code"], ["usb", "From USB"], ["saved", "Saved"]] as [Tab, string][]).map(([t, label]) => (
            <button key={t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>{label}</button>
          ))}
        </nav>
        {tab === "pair" && <PairTab onChanged={onChanged} />}
        {tab === "qr" && <QrTab onChanged={onChanged} />}
        {tab === "usb" && <UsbTab devices={devices} onChanged={onChanged} />}
        {tab === "saved" && <SavedTab onChanged={onChanged} />}
        <div className="modal-actions"><button onClick={onClose}>Close</button></div>
      </div>
    </div>
  );
}

function PairTab({ onChanged }: { onChanged: Props["onChanged"] }) {
  const [pairAddr, setPairAddr] = useState("");
  const [code, setCode] = useState("");
  const [connAddr, setConnAddr] = useState("");
  const [found, setFound] = useState<MdnsService[]>([]);
  const [status, setStatus] = useState<Status>(null);
  const [busy, setBusy] = useState(false);

  const discover = useCallback(async (quiet = false) => {
    if (!quiet) setStatus({ kind: "busy", text: "Looking for phones on this network…" });
    try {
      const v = await ipc.wireless.discover();
      setFound(v);
      const pairing = v.find((s) => s.kind === "pairing");
      if (pairing) setPairAddr((cur) => cur || pairing.address);
      const conn = v.find((s) => s.kind === "connect");
      if (conn) setConnAddr((cur) => cur || conn.address);
      if (!quiet) setStatus(v.length ? { kind: "ok", text: `Found ${v.length} service(s). Address fields were filled in.` } : { kind: "error", text: "Nothing found. Open the pairing dialog on the phone, or type the address shown there." });
    } catch (e) { if (!quiet) setStatus({ kind: "error", text: errText(e) }); }
  }, []);

  const run = async (fn: () => Promise<void>) => { setBusy(true); try { await fn(); } catch (e) { setStatus({ kind: "error", text: errText(e) }); } finally { setBusy(false); } };

  const pair = () => run(async () => {
    setStatus({ kind: "busy", text: "Pairing…" });
    await ipc.wireless.pair(pairAddr.trim(), code.trim());
    setCode("");
    setStatus({ kind: "ok", text: "Paired. Now connect using the port under Wireless debugging (looking for it…)." });
    const v = await ipc.wireless.discover().catch(() => []);
    const conn = v.find((s) => s.kind === "connect" && s.address.startsWith(ipOf(pairAddr.trim()) + ":"));
    if (conn) {
      setConnAddr(conn.address);
      const msg = await ipc.wireless.connect(conn.address);
      setStatus({ kind: "ok", text: `Paired and ${msg}` });
      onChanged(conn.address);
    }
  });

  const connect = () => run(async () => {
    setStatus({ kind: "busy", text: "Connecting…" });
    const msg = await ipc.wireless.connect(connAddr.trim());
    setStatus({ kind: "ok", text: msg });
    onChanged(connAddr.trim());
  });

  useEffect(() => { discover(true); }, [discover]);

  return (
    <div className="tabbody">
      <ol className="steps">
        <li>Phone and computer on the same Wi-Fi.</li>
        <li>Phone: Settings &gt; Developer options &gt; <b>Wireless debugging</b> (Android 11 or newer) &gt; turn on.</li>
        <li>Tap <b>Pair device with pairing code</b>. Enter the address and 6-digit code below. Pairing is needed once per computer.</li>
        <li>Then connect using the address shown on the main Wireless debugging screen (the port differs from the pairing port).</li>
      </ol>
      <div className="row"><button onClick={() => discover()} disabled={busy}>Find phones on network</button>
        {found.length > 0 && <span className="muted">{found.length} found</span>}</div>
      <div className="grid2">
        <label>Pairing address<input value={pairAddr} onChange={(e) => setPairAddr(e.target.value)} placeholder="192.168.1.20:37099" inputMode="decimal" /></label>
        <label>6-digit code<input value={code} onChange={(e) => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))} placeholder="123456" inputMode="numeric" autoComplete="off" /></label>
      </div>
      <div className="row"><button className="primary" onClick={pair} disabled={busy || !pairAddr || code.length !== 6}>Pair</button></div>
      <div className="grid2">
        <label>Connect address<input value={connAddr} onChange={(e) => setConnAddr(e.target.value)} placeholder="192.168.1.20:41231" inputMode="decimal" /></label>
        <div className="row end"><button className="primary" onClick={connect} disabled={busy || !connAddr}>Connect</button></div>
      </div>
      <StatusLine s={status} />
    </div>
  );
}

function QrTab({ onChanged }: { onChanged: Props["onChanged"] }) {
  const [qr, setQr] = useState<QrStart | null>(null);
  const [status, setStatus] = useState<Status>(null);
  const live = useRef(true);

  const start = useCallback(async () => {
    setStatus({ kind: "busy", text: "Waiting for the phone to scan the code…" });
    try {
      const q = await ipc.wireless.qrStart();
      if (!live.current) return;
      setQr(q);
      const msg = await ipc.wireless.qrWait(q);
      if (!live.current) return;
      setStatus({ kind: "ok", text: msg });
      const m = /(\d+\.\d+\.\d+\.\d+:\d+)/.exec(msg);
      onChanged(m?.[1]);
    } catch (e) { if (live.current) { setQr(null); setStatus({ kind: "error", text: errText(e) }); } }
  }, [onChanged]);

  useEffect(() => {
    live.current = true;
    return () => { live.current = false; ipc.wireless.qrCancel().catch(() => {}); };
  }, []);

  return (
    <div className="tabbody">
      <ol className="steps">
        <li>Phone: Settings &gt; Developer options &gt; <b>Wireless debugging</b> &gt; <b>Pair device with QR code</b>.</li>
        <li>Press Start here, then scan the code with the phone. Pairing and connecting happen automatically.</li>
      </ol>
      <div className="row"><button className="primary" onClick={start} disabled={!!qr}>Start</button>
        {qr && <button onClick={() => { ipc.wireless.qrCancel(); setQr(null); setStatus(null); }}>Cancel</button>}</div>
      {/* The SVG is generated locally by the app's own backend, not from remote input. */}
      {qr && <div className="qr" aria-label="Pairing QR code" dangerouslySetInnerHTML={{ __html: qr.svg }} />}
      <StatusLine s={status} />
    </div>
  );
}

function UsbTab({ devices, onChanged }: { devices: Device[]; onChanged: Props["onChanged"] }) {
  const usb = devices.filter((d) => d.state === "device" && !isWireless(d.serial));
  const [serial, setSerial] = useState(usb[0]?.serial ?? "");
  const [status, setStatus] = useState<Status>(null);
  const [busy, setBusy] = useState(false);

  const go = async () => {
    setBusy(true); setStatus({ kind: "busy", text: "Switching the phone to Wi-Fi mode…" });
    try {
      const address = await ipc.wireless.usbToWireless(serial);
      setStatus({ kind: "ok", text: `Connected to ${address}. You can unplug the cable. This resets when the phone restarts.` });
      onChanged(address);
    } catch (e) { setStatus({ kind: "error", text: errText(e) }); } finally { setBusy(false); }
  };

  return (
    <div className="tabbody">
      <p>Works on any Android version. Plug the phone in once with USB debugging on, switch it to Wi-Fi, then unplug.</p>
      {usb.length === 0 ? <p className="muted">No USB device is connected and authorised right now.</p> : (
        <div className="row">
          <select value={serial} onChange={(e) => setSerial(e.target.value)} aria-label="USB device">
            {usb.map((d) => <option key={d.serial} value={d.serial}>{d.model ?? d.serial} — {d.serial}</option>)}
          </select>
          <button className="primary" onClick={go} disabled={busy || !serial}>Switch to Wi-Fi</button>
        </div>
      )}
      <StatusLine s={status} />
    </div>
  );
}

function SavedTab({ onChanged }: { onChanged: Props["onChanged"] }) {
  const [list, setList] = useState<SavedDevice[]>([]);
  const [status, setStatus] = useState<Status>(null);
  const [busy, setBusy] = useState("");
  const load = useCallback(() => ipc.wireless.saved().then(setList).catch(() => {}), []);
  useEffect(() => { load(); }, [load]);

  const reconnect = async (d: SavedDevice) => {
    setBusy(d.address); setStatus({ kind: "busy", text: `Connecting to ${d.name}…` });
    try { const msg = await ipc.wireless.reconnect(d.address); setStatus({ kind: "ok", text: msg }); onChanged(); await load(); }
    catch (e) { setStatus({ kind: "error", text: errText(e) }); } finally { setBusy(""); }
  };

  return (
    <div className="tabbody">
      {list.length === 0 && <p className="muted">Phones you connect over Wi-Fi are remembered here. The app finds the new port automatically if it changed.</p>}
      {list.map((d) => (
        <div key={d.address} className="saved">
          <div><b>{d.name}</b><div className="muted mono">{d.address}</div></div>
          <div className="row">
            <button className="primary" disabled={!!busy} onClick={() => reconnect(d)}>{busy === d.address ? "…" : "Reconnect"}</button>
            <button disabled={!!busy} onClick={async () => { await ipc.wireless.forget(d.address); load(); }}>Forget</button>
          </div>
        </div>
      ))}
      <StatusLine s={status} />
    </div>
  );
}
