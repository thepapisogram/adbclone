import { updateSettings, useSettings, type ThemeChoice } from "../lib/settings";
import { LANGUAGES, useT } from "../lib/i18n";

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const s = useSettings();
  const t = useT();
  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal" role="dialog" aria-label="Settings" onClick={(e) => e.stopPropagation()}>
        <h2>{t("settings.title")}</h2>
        <label className="setting">
          <span><b>{t("settings.theme")}</b><br /><span className="muted small">{t("settings.themeHelp")}</span></span>
          <select value={s.theme} onChange={(e) => updateSettings({ theme: e.target.value as ThemeChoice })} aria-label="Theme">
            <option value="system">{t("settings.themeSystem")}</option>
            <option value="light">{t("settings.themeLight")}</option>
            <option value="dark">{t("settings.themeDark")}</option>
          </select>
        </label>
        <label className="setting">
          <span><b>{t("settings.technical")}</b><br /><span className="muted small">{t("settings.technicalHelp")}</span></span>
          <input type="checkbox" role="switch" checked={!s.plain} onChange={(e) => updateSettings({ plain: !e.target.checked })} aria-label="Show technical details" />
        </label>
        <label className="setting">
          <span><b>{t("settings.language")}</b><br /><span className="muted small">{t("settings.languageHelp")}</span></span>
          <select value={s.language.split("-")[0]} onChange={(e) => updateSettings({ language: e.target.value })} aria-label={t("settings.language")}>
            {LANGUAGES.map((l) => <option key={l.code} value={l.code}>{l.name}</option>)}
          </select>
        </label>
        <div className="modal-actions"><button className="primary" onClick={onClose}>{t("common.done")}</button></div>
      </div>
    </div>
  );
}
