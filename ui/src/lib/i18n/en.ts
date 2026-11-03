/** English is the source of truth: every other language is checked against these keys. `{n}` style placeholders are filled in by `t`. */
export const en = {
  "tab.apps": "Apps", "tab.running": "Running", "tab.health": "Temperature", "tab.debloat": "Debloat", "tab.tune": "Tune",
  "tab.install": "Install", "tab.storage": "Storage", "tab.ratings": "Ratings", "tab.phones": "Phones", "tab.wake": "Wake-ups",
  "tab.snapshots": "Snapshots", "tab.logs": "Logs",
  "header.refresh": "Refresh", "header.connect": "Connect…", "header.disconnect": "Disconnect", "header.help": "Getting started", "header.settings": "Settings",
  "header.noDevices": "No devices", "header.selectDevice": "Select device…",
  "action.disable": "Disable", "action.enable": "Enable", "action.uninstall": "Uninstall", "action.restore": "Restore", "action.stop": "Force stop",
  "common.cancel": "Cancel", "common.done": "Done", "common.working": "Working…", "common.reload": "Reload", "common.loading": "Loading…",
  "apps.search": "Search name or package…", "apps.allApps": "All apps", "apps.system": "System", "apps.user": "User",
  "apps.anyState": "Any state", "apps.enabled": "Enabled", "apps.disabled": "Disabled", "apps.uninstalled": "Uninstalled",
  "apps.anyRisk": "Any risk", "apps.recommended": "Recommended", "apps.advanced": "Advanced", "apps.unsafe": "Unsafe", "apps.unrated": "Unrated",
  "confirm.one": "{action} {n} app?", "confirm.many": "{action} {n} apps?", "confirm.alsoOn": "Also do this on",
  "settings.title": "Settings", "settings.theme": "Theme", "settings.themeHelp": "Follow your computer, or choose.",
  "settings.themeSystem": "Same as my computer", "settings.themeLight": "Light", "settings.themeDark": "Dark",
  "settings.technical": "Show technical details", "settings.technicalHelp": "Package names and other developer text. Off keeps things in plain words.",
  "settings.language": "Language", "settings.languageHelp": "Translations are drafts; English is used for anything not yet translated.",
  "empty.selectDevice": "Select a device to see its apps.",
} as const;
export type Key = keyof typeof en;
