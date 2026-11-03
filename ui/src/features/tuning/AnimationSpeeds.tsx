import { useEffect, useState } from "react";
import { PRESETS, SCALES, currentPreset, nearestScale, speedWord, type Mix, type Scale } from "./animations";
import type { TuningStatus } from "../../lib/types";

interface Props {
  status?: TuningStatus;
  busy: boolean;
  onApply: (m: Mix) => void;
  onReset: () => void;
  changed: boolean;
}

const fields: { key: keyof Mix; label: string; help: string }[] = [
  { key: "window", label: "Windows opening and closing", help: "Apps and dialogs appearing." },
  { key: "transition", label: "Moving between screens", help: "Going into and out of a screen." },
  { key: "animator", label: "Motion inside apps", help: "Scrolling effects, buttons, menus." },
];

export function AnimationSpeeds({ status, busy, onApply, onReset, changed }: Props) {
  const fromPhone = (): Mix => ({ window: nearestScale(status?.animationScale), transition: nearestScale(status?.transitionScale), animator: nearestScale(status?.animatorScale) });
  const [mix, setMix] = useState<Mix>(fromPhone);
  useEffect(() => { setMix(fromPhone()); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [status?.animationScale, status?.transitionScale, status?.animatorScale]);
  const now = currentPreset(status?.animationScale, status?.transitionScale, status?.animatorScale);
  const dirty = JSON.stringify(mix) !== JSON.stringify(fromPhone());

  return (
    <section className="block" aria-label="Animation speeds">
      <h3>Animation speeds</h3>
      <p className="muted small">
        Shorter animations make the phone feel quicker and give the graphics chip less to do. These are Android's own developer settings: nothing is installed, and "Back to normal" restores what you had.
        {" "}Now: <b>{now ? now.title : "a custom mix"}</b>.
      </p>
      <div className="presetgrid">
        {PRESETS.map((p) => (
          <button key={p.id} className={`presetcard ${now?.id === p.id ? "current" : ""}`} disabled={busy || !status} onClick={() => onApply(p.mix)} aria-label={`Use ${p.title}`} aria-pressed={now?.id === p.id}>
            <b>{p.title}</b>
            <span className="muted small">{p.text}</span>
            <span className="small mono">{p.mix.window} / {p.mix.transition} / {p.mix.animator}</span>
          </button>
        ))}
      </div>
      <h4>Make your own mix</h4>
      <div className="mixgrid">
        {fields.map((f) => (
          <label key={f.key} className="mixrow">
            <span><b>{f.label}</b><br /><span className="muted small">{f.help}</span></span>
            <select value={mix[f.key]} disabled={busy || !status} aria-label={f.label} onChange={(e) => setMix({ ...mix, [f.key]: e.target.value as Scale })}>
              {SCALES.map((s) => <option key={s} value={s}>{s}× ({speedWord(s)})</option>)}
            </select>
          </label>
        ))}
      </div>
      <div className="row">
        <button className="primary" disabled={busy || !status || !dirty} onClick={() => onApply(mix)}>Apply this mix</button>
        <button disabled={busy || !changed} onClick={onReset}>Back to normal</button>
      </div>
    </section>
  );
}
