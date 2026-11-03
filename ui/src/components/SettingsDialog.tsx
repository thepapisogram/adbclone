import { updateSettings, useSettings, type ThemeChoice } from "../lib/settings";

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const s = useSettings();
  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal" role="dialog" aria-label="Settings" onClick={(e) => e.stopPropagation()}>
        <h2>Settings</h2>
        <label className="setting">
          <span><b>Theme</b><br /><span className="muted small">Follow your computer, or choose.</span></span>
          <select value={s.theme} onChange={(e) => updateSettings({ theme: e.target.value as ThemeChoice })} aria-label="Theme">
            <option value="system">Same as my computer</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </label>
        <label className="setting">
          <span><b>Show technical details</b><br /><span className="muted small">Package names and other developer text. Off keeps things in plain words.</span></span>
          <input type="checkbox" role="switch" checked={!s.plain} onChange={(e) => updateSettings({ plain: !e.target.checked })} aria-label="Show technical details" />
        </label>
        <div className="modal-actions"><button className="primary" onClick={onClose}>Done</button></div>
      </div>
    </div>
  );
}
