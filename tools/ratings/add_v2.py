#!/usr/bin/env python3
"""Adds ratings (and duplicate-of links) to src-tauri/data/ratings. Safe to run repeatedly: ids already present are left alone.
Only package names I am confident exist are listed; every entry carries a confidence level and is plain guidance, not a promise."""
import glob, importlib.util, json, os, sys

DIR = "src-tauri/data/ratings"
spec = importlib.util.spec_from_file_location("migrate_v2", os.path.join(os.path.dirname(__file__), "migrate_v2.py"))
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
IMPACT, TIER_BY_CAT, REINSTALL = m.IMPACT, m.TIER_BY_CAT, m.REINSTALL
IMPACT["carrier-family"] = "Carrier apps can include network setup (calls, data, Wi-Fi calling, voicemail). Only remove ones you recognise."
IMPACT["core-family"] = "Part of the phone's core software or radio. Never offered for removal."

R, A, U = "recommended", "advanced", "unsafe"
# (file, id, risk, category, confidence, description[, extra])
# tier comes from the category unless extra["tier"] is given; extra may hold prefix / duplicate_of / impact / vendor.
NEW = []
def add(file, rows):
    for r in rows: NEW.append((file,) + r)

# ---- core software that must never be offered: whole families (prefix rules) ----
FAMILIES = [
    ("com.android.providers.", "Content providers (contacts, SMS, downloads, media...)."), ("com.android.server.", "System server components."),
    ("com.android.systemui.", "System UI plugins and overlays."), ("com.android.internal.", "Internal system overlays."),
    ("com.android.theme.", "System theme overlays."), ("com.android.overlay.", "System overlays."), ("com.android.cts.", "Compatibility shims."),
    ("com.android.wifi.", "Wi-Fi components."), ("com.android.connectivity.", "Connectivity components."), ("com.android.bluetooth.", "Bluetooth components."),
    ("com.android.nfc.", "NFC components."), ("com.android.phone.", "Telephony components."), ("com.android.networkstack.", "Network stack components."),
    ("com.google.android.overlay.", "Google system overlays."), ("android.auto_generated_rro_", "Generated system overlays."), ("com.google.mainline.", "Updatable system modules."),
    ("com.google.android.tzdata", "Time zone data."), ("com.google.android.conscrypt", "Security library module."), ("com.google.android.adservices", "Privacy sandbox module."),
    ("com.google.android.gms.", "Parts of Google Play services."), ("com.google.android.gsf.", "Parts of the Google services framework."),
    ("com.sec.android.provider.", "Samsung content providers."), ("com.samsung.android.provider.", "Samsung content providers."),
    ("com.qualcomm.", "Qualcomm chip and radio services (specific harmless ones are rated separately)."), ("com.qti.", "Qualcomm chip services."),
    ("vendor.qti.", "Qualcomm vendor services."), ("org.codeaurora.", "Qualcomm open-source services."), ("com.mediatek.", "MediaTek chip and radio services (specific harmless ones are rated separately)."),
    ("com.sprd.", "Spreadtrum/Unisoc chip services."), ("com.unisoc.", "Unisoc chip services."),
]
for pre, desc in FAMILIES:
    add("android-core" if pre.startswith(("com.android", "android.")) else "misc" if not pre.startswith(("com.google", "com.sec", "com.samsung")) else "google" if pre.startswith("com.google") else "samsung",
        [(pre, U, "core-system", "high", desc, {"prefix": True})])

# ---- Google ----
add("google", [
    ("com.google.android.apps.youtube.kids", R, "google-app", "medium", "YouTube Kids."), ("com.google.android.apps.kids.familylinkhelper", A, "system-service", "medium", "Family Link helper (parental controls)."),
    ("com.google.android.apps.googlevoice", R, "google-app", "medium", "Google Voice."), ("com.google.android.apps.dynamite", R, "google-app", "medium", "Google Chat."),
    ("com.google.android.apps.nbu.paisa.user", R, "google-app", "medium", "Google Pay (India)."), ("com.google.android.apps.healthdata", A, "system-service", "medium", "Health Connect."),
    ("com.google.android.apps.safetyhub", R, "google-app", "medium", "Personal Safety."), ("com.google.android.apps.adm", A, "system-service", "medium", "Find My Device."),
    ("com.google.android.apps.searchlite", R, "google-app", "medium", "Google Go."), ("com.google.android.apps.mapslite", R, "google-app", "medium", "Google Maps Go."),
    ("com.google.android.apps.cameralite", R, "google-app", "medium", "Camera Go."), ("com.google.android.apps.books", R, "google-app", "medium", "Google Play Books."),
    ("com.google.android.apps.wallpaper", R, "google-app", "medium", "Wallpapers."), ("com.google.android.apps.photosgo", R, "google-app", "medium", "Gallery Go."),
    ("com.google.android.calculator", R, "google-app", "medium", "Google Calculator."), ("com.google.android.deskclock", A, "core-app", "medium", "Google Clock (alarms and timers)."),
    ("com.google.android.GoogleCamera", A, "core-app", "medium", "Google Camera."), ("com.google.android.gm.exchange", A, "system-service", "medium", "Exchange account support for Gmail."),
    ("com.google.android.ims", A, "system-service", "medium", "Carrier Services (RCS chat features)."), ("com.google.android.apps.pixel.setupwizard", A, "system-service", "medium", "Pixel setup."),
    ("com.google.android.apps.work.clouddpc", A, "system-service", "medium", "Android Device Policy (work devices)."), ("com.google.android.apps.nexuslauncher", U, "core-system", "high", "Pixel launcher."),
    ("com.google.android.apps.tasks", R, "google-app", "medium", "Google Tasks."), ("com.google.android.apps.youtube.creator", R, "google-app", "medium", "YouTube Studio."),
    ("com.google.android.youtube.tv", R, "google-app", "medium", "YouTube for TV."), ("com.google.android.apps.recorder", R, "google-app", "medium", "Pixel Recorder."),
    ("com.google.android.apps.pixel.psi", A, "system-service", "low", "Pixel system intelligence helper."), ("com.google.android.as", A, "system-service", "medium", "Device Personalization Services (suggestions, Now Playing)."),
    ("com.google.android.apps.turbo", A, "system-service", "medium", "Device Health Services (battery tips)."), ("com.google.android.googlequicksearchbox", A, "core-app", "high", "Google app."),
    ("com.google.android.projection.gearhead", R, "google-app", "medium", "Android Auto."), ("com.google.android.apps.assistant", R, "assistant", "low", "Assistant helper."),
    ("com.google.android.apps.messaging", A, "core-app", "high", "Google Messages."), ("com.google.android.apps.carrier.log", R, "ads-telemetry", "medium", "Carrier diagnostics logging."),
    ("com.google.android.apps.gcs", A, "system-service", "low", "Google Connectivity Services."), ("com.google.android.apps.stargazer", R, "google-app", "low", "Stargazer."),
])
# ---- Samsung ----
add("samsung", [
    ("com.samsung.android.app.galaxyfinder", A, "vendor-app", "medium", "Finder (search on the phone)."), ("com.samsung.android.app.cocktailbarservice", A, "vendor-app", "medium", "Edge panels."),
    ("com.samsung.android.app.taskedge", A, "vendor-app", "medium", "Edge panel: Tasks."), ("com.samsung.android.app.clipboardedge", A, "vendor-app", "medium", "Edge panel: Clipboard."),
    ("com.samsung.android.service.peoplestripe", A, "vendor-app", "medium", "Edge panel: People."), ("com.samsung.android.app.appsedge", A, "vendor-app", "medium", "Edge panel: Apps."),
    ("com.samsung.android.dynamiclock", R, "vendor-app", "medium", "Dynamic Lock Screen (changing wallpapers and news)."), ("com.samsung.android.app.omcagent", A, "system-service", "medium", "Carrier customisation (OMC)."),
    ("com.samsung.android.mcfserver", A, "system-service", "low", "Continuity service."), ("com.samsung.android.aware.service", A, "system-service", "medium", "Nearby device scanning."),
    ("com.samsung.android.allshare.service.mediashare", R, "vendor-app", "medium", "Media sharing to TVs."), ("com.samsung.android.allshare.service.fileshare", R, "vendor-app", "medium", "File sharing."),
    ("com.sec.android.easyMover", R, "vendor-app", "medium", "Smart Switch."), ("com.sec.android.easyMover.Agent", R, "vendor-app", "medium", "Smart Switch helper."),
    ("com.sec.android.app.voicenote", R, "vendor-app", "medium", "Voice Recorder."), ("com.sec.android.app.billing", A, "payments", "low", "Samsung Checkout."),
    ("com.sec.android.widgetapp.samsungapps", R, "store", "medium", "Galaxy Store widget."), ("com.sec.android.app.kidshome", R, "vendor-app", "medium", "Kids Home."),
    ("com.samsung.android.app.sbrowseredge", R, "vendor-app", "medium", "Samsung Internet edge helper."), ("com.samsung.android.smartsuggestions", A, "vendor-app", "low", "Smart suggestions."),
    ("com.sec.android.app.shealth", R, "vendor-app", "medium", "Samsung Health."), ("com.samsung.android.visioncloudagent", R, "ads-telemetry", "low", "Vision cloud agent."),
    ("com.samsung.android.app.watchmanagerstub", R, "partner-stub", "medium", "Galaxy Wearable installer stub."), ("com.samsung.android.shortcutbackupservice", A, "system-service", "low", "Home screen shortcut backup."),
    ("com.samsung.android.app.dofviewer", R, "vendor-app", "low", "Photo depth viewer."), ("com.samsung.android.bixby.ondevice.enus", R, "assistant", "low", "Bixby language pack (English)."),
    ("com.samsung.android.bixbyvision.framework", R, "assistant", "low", "Bixby Vision framework."), ("com.samsung.android.wcmurlsnetworkstack", A, "system-service", "low", "Network helper."),
    ("com.samsung.android.net.wifi.wifiguider", R, "vendor-app", "low", "Wi-Fi guider."), ("com.samsung.android.hmt.vrsvc", R, "vendor-app", "low", "Gear VR service."),
    ("com.samsung.android.hmt.vrshell", R, "vendor-app", "low", "Gear VR shell."), ("com.samsung.android.ardrawing", R, "vendor-app", "medium", "AR Doodle."),
    ("com.sec.android.autodoodle.service", R, "vendor-app", "low", "AR Emoji doodle service."), ("com.samsung.android.tvplus", R, "preinstalled-app", "medium", "Samsung TV Plus."),
    ("com.samsung.android.app.spage", R, "vendor-app", "medium", "Samsung Free."), ("com.samsung.android.mapsagent", R, "ads-telemetry", "low", "Maps agent."),
    ("com.samsung.android.mdecservice", A, "system-service", "low", "Mobile device extension service."), ("com.samsung.android.ConnectivityOverlay", U, "core-system", "high", "Connectivity overlay."),
    ("com.sec.android.app.safetyassurance", A, "system-service", "low", "Safety assurance."), ("com.samsung.android.da.daagent", R, "ads-telemetry", "medium", "Diagnostics agent."),
])
# ---- Xiaomi / Redmi / POCO ----
add("xiaomi", [
    ("com.mi.android.globalminusscreen", R, "vendor-app", "medium", "App Vault (the left-hand screen)."), ("com.miui.backup", R, "vendor-app", "medium", "Backup."),
    ("com.miui.cleanmaster", R, "vendor-app", "low", "Cleaner."), ("com.miui.global.packageinstaller", A, "core-app", "medium", "Package installer."),
    ("com.xiaomi.misettings", U, "core-system", "high", "Xiaomi settings."), ("com.miui.securitycenter", A, "system-service", "medium", "Security app."),
    ("com.miui.miwallpaper.snowmountain", R, "vendor-app", "low", "Super wallpaper."), ("com.miui.miwallpaper.earth", R, "vendor-app", "low", "Super wallpaper."),
    ("com.miui.miwallpaper.mars", R, "vendor-app", "low", "Super wallpaper."), ("com.miui.wallpaper.overlay", A, "system-service", "low", "Wallpaper overlay."),
    ("com.xiaomi.mi_connect_service", R, "vendor-app", "medium", "Mi Connect."), ("com.miui.audiomonitor", R, "ads-telemetry", "low", "Audio monitor."),
    ("com.miui.guardprovider", A, "system-service", "low", "Security scan provider."), ("com.xiaomi.mipicks", R, "store", "medium", "GetApps."),
    ("com.miui.freeform", A, "system-service", "low", "Floating windows."), ("com.miui.mishare.connectivity", R, "vendor-app", "medium", "Mi Share."),
    ("com.xiaomi.gamecenter", R, "game-hub", "low", "Game Center."), ("com.mi.globalTrendNews", R, "ads-telemetry", "low", "News feed."),
    ("com.miui.vpnsdkmanager", A, "system-service", "low", "VPN helper."), ("com.xiaomi.xmsf", A, "system-service", "medium", "Xiaomi services framework."),
    ("com.miui.cit", R, "ads-telemetry", "low", "Hardware test tool."), ("com.miui.qr", R, "vendor-app", "low", "QR scanner."),
])
# ---- Huawei / Honor ----
add("huawei-honor", [
    ("com.huawei.hiview", R, "ads-telemetry", "medium", "Diagnostic and log service."), ("com.huawei.android.pushagent", A, "system-service", "medium", "Push messages."),
    ("com.huawei.vassistant", R, "assistant", "medium", "Celia voice assistant."), ("com.huawei.hiai", A, "system-service", "low", "HiAI engine."),
    ("com.huawei.android.thememanager", R, "vendor-app", "medium", "Themes."), ("com.huawei.hivoice", R, "assistant", "low", "Voice engine."),
    ("com.huawei.camera", A, "core-app", "medium", "Camera."), ("com.huawei.photos", A, "core-app", "medium", "Gallery."),
    ("com.huawei.contacts", A, "core-app", "medium", "Contacts."), ("com.huawei.android.launcher", U, "core-system", "high", "Home screen."),
    ("com.huawei.appmarket", R, "store", "medium", "AppGallery."), ("com.huawei.hwid", A, "cloud-sync", "medium", "Huawei ID."),
    ("com.huawei.hicloud", A, "cloud-sync", "low", "Huawei Cloud."), ("com.huawei.parentcontrol", R, "vendor-app", "low", "Parental controls."),
    ("com.huawei.desktop.explorer", R, "vendor-app", "low", "Desktop mode helper."), ("com.huawei.search", R, "vendor-app", "low", "Search."),
])
# ---- Oppo / Realme / OnePlus ----
add("oppo-realme-oneplus", [
    ("com.oplus.statistics.rom", R, "ads-telemetry", "medium", "Usage statistics."), ("com.nearme.statistics.rom", R, "ads-telemetry", "medium", "Usage statistics."),
    ("com.oplus.postmanservice", R, "ads-telemetry", "low", "Feedback and diagnostics."), ("com.heytap.mcs", A, "system-service", "medium", "Push service."),
    ("com.heytap.usercenter", A, "cloud-sync", "medium", "HeyTap account."), ("com.heytap.themestore", R, "vendor-app", "low", "Theme store."),
    ("com.coloros.gallery3d", A, "core-app", "medium", "Gallery."), ("com.coloros.filemanager", A, "core-app", "medium", "File manager."),
    ("com.coloros.alarmclock", A, "core-app", "medium", "Clock."), ("com.coloros.calendar", R, "vendor-app", "medium", "Calendar."),
    ("com.coloros.screenrecorder", R, "vendor-app", "medium", "Screen recorder."), ("com.coloros.oppomultiapp", R, "vendor-app", "low", "Clone apps."),
    ("com.coloros.backuprestore", R, "vendor-app", "medium", "Clone Phone."), ("com.coloros.sceneservice", A, "system-service", "low", "Smart scenes."),
    ("com.oplus.athena", A, "system-service", "low", "System optimiser."), ("com.realme.link", R, "vendor-app", "medium", "Realme Link."),
    ("com.heytap.cloud", A, "cloud-sync", "medium", "HeyTap Cloud."), ("com.oneplus.opbugreport", R, "ads-telemetry", "low", "Bug report tool."),
    ("com.oneplus.aod", R, "vendor-app", "low", "Ambient display."), ("com.oneplus.iconpack.oneplus", R, "aosp-extra", "low", "Icon pack."),
    ("com.oneplus.launcher.themes", R, "aosp-extra", "low", "Launcher themes."), ("com.oplus.games", R, "game-hub", "medium", "Game assistant."),
    ("com.coloros.healthservice", R, "vendor-app", "low", "Health service."), ("com.coloros.speechassist", R, "assistant", "low", "Breeno voice assistant."),
])
# ---- vivo / iQOO ----
add("vivo-iqoo", [
    ("com.vivo.browser", R, "vendor-app", "medium", "vivo Browser."), ("com.vivo.appstore", R, "store", "medium", "V-Appstore."),
    ("com.bbk.account", A, "cloud-sync", "medium", "vivo Account."), ("com.vivo.email", R, "vendor-app", "medium", "Email."),
    ("com.vivo.weather", R, "vendor-app", "medium", "Weather."), ("com.vivo.game", R, "game-hub", "medium", "Game Center."),
    ("com.vivo.gamecube", R, "game-hub", "medium", "Game Cube."), ("com.vivo.easyshare", R, "vendor-app", "medium", "EasyShare."),
    ("com.vivo.space", R, "help-promo", "medium", "vivo community app."), ("com.vivo.floatingball", R, "vendor-app", "low", "Floating ball shortcut."),
    ("com.vivo.agent", R, "assistant", "medium", "Jovi voice assistant."), ("com.vivo.assistant", R, "assistant", "medium", "Jovi assistant."),
    ("com.vivo.childrenmode", R, "vendor-app", "low", "Kids Space."), ("com.android.bbkcalculator", R, "vendor-app", "medium", "Calculator."),
    ("com.android.BBKClock", A, "core-app", "medium", "Clock."), ("com.android.notes", R, "vendor-app", "medium", "Notes."),
    ("com.vivo.pushservice", A, "system-service", "low", "Push service."), ("com.vivo.upslide", R, "vendor-app", "low", "Swipe-up shortcut panel."),
])
# ---- Transsion (Tecno, Infinix, itel) ----
add("transsion", [
    ("com.transsion.carlcare", R, "help-promo", "medium", "Carlcare support app."), ("com.transsion.hilauncher", U, "core-system", "high", "HiOS launcher."),
    ("com.transsion.XOSLauncher", U, "core-system", "high", "XOS launcher."), ("com.transsion.phonemaster", A, "system-service", "medium", "Phone Master (cleaner, security)."),
    ("com.transsion.theme", R, "vendor-app", "low", "Themes."), ("com.afmobi.boomplayer", R, "preinstalled-app", "medium", "Boomplay music."),
    ("com.transsion.smartpanel", R, "vendor-app", "low", "Smart panel."), ("com.transsion.magicshow", R, "vendor-app", "low", "Video effects."),
])
# ---- Asus / LG / Sony / Motorola extras ----
add("misc", [
    ("com.asus.launcher", U, "core-system", "high", "ZenUI launcher."), ("com.asus.mobilemanager", A, "system-service", "medium", "Mobile Manager."),
    ("com.asus.gamewidget", R, "game-hub", "medium", "Game Genie."), ("com.asus.weathertime", R, "vendor-app", "medium", "Weather and clock widget."),
    ("com.asus.calculator", R, "vendor-app", "medium", "Calculator."), ("com.asus.soundrecorder", R, "vendor-app", "medium", "Sound Recorder."),
    ("com.asus.ia.asusapp", R, "help-promo", "medium", "MyASUS."), ("com.asus.camera", A, "core-app", "medium", "Camera."),
    ("com.asus.gallery", A, "core-app", "medium", "Gallery."), ("com.asus.filemanager", A, "core-app", "medium", "File manager."),
    ("com.asus.browser", R, "vendor-app", "medium", "Browser."), ("com.asus.deskclock", A, "core-app", "medium", "Clock."),
    ("com.lge.launcher3", U, "core-system", "high", "LG launcher."), ("com.lge.lgworld", R, "store", "low", "LG World."),
    ("com.lge.smartshare", R, "vendor-app", "low", "SmartShare."), ("com.lge.qmemoplus", R, "vendor-app", "medium", "QuickMemo+."),
    ("com.lge.music", R, "vendor-app", "medium", "Music."), ("com.lge.email", R, "vendor-app", "medium", "Email."),
    ("com.sonymobile.android.dialer", A, "core-app", "low", "Sony dialer."), ("com.sonymobile.xperiaxlivewallpaper", R, "aosp-extra", "low", "Live wallpaper."),
    ("com.motorola.camera3", A, "core-app", "medium", "Camera."), ("com.motorola.audiofx", R, "vendor-app", "low", "Audio effects."),
])
# ---- Preinstalled third parties (often promotional installs) ----
add("social-partners", [
    ("com.zhiliaoapp.musically", R, "preinstalled-app", "medium", "TikTok."), ("com.ss.android.ugc.trill", R, "preinstalled-app", "medium", "TikTok (Asia)."),
    ("com.facebook.lite", R, "preinstalled-app", "medium", "Facebook Lite."), ("com.facebook.mlite", R, "preinstalled-app", "medium", "Messenger Lite."),
    ("com.snapchat.android", R, "preinstalled-app", "medium", "Snapchat."), ("com.twitter.android", R, "preinstalled-app", "medium", "X (Twitter)."),
    ("com.pinterest", R, "preinstalled-app", "medium", "Pinterest."), ("com.booking", R, "preinstalled-app", "medium", "Booking.com."),
    ("com.agoda.mobile.consumer", R, "preinstalled-app", "low", "Agoda."), ("com.ebay.mobile", R, "preinstalled-app", "medium", "eBay."),
    ("com.microsoft.teams", R, "preinstalled-app", "medium", "Microsoft Teams."), ("com.microsoft.todos", R, "preinstalled-app", "medium", "Microsoft To Do."),
    ("com.microsoft.bing", R, "preinstalled-app", "medium", "Microsoft Bing."), ("com.microsoft.launcher", A, "preinstalled-app", "low", "Microsoft Launcher."),
    ("com.opera.mini.native", R, "preinstalled-app", "medium", "Opera Mini."), ("com.opera.browser", R, "preinstalled-app", "medium", "Opera."),
    ("in.startv.hotstar", R, "preinstalled-app", "medium", "Hotstar."), ("net.one97.paytm", R, "preinstalled-app", "medium", "Paytm."),
    ("com.phonepe.app", R, "preinstalled-app", "medium", "PhonePe."), ("com.truecaller", R, "preinstalled-app", "medium", "Truecaller."),
    ("com.gaana", R, "preinstalled-app", "medium", "Gaana."), ("com.bsbportal.music", R, "preinstalled-app", "medium", "Wynk Music."),
    ("com.king.candycrushjellysaga", R, "game-hub", "medium", "Candy Crush Jelly Saga."), ("com.gameloft.android.ANMP.GloftA9HM", R, "game-hub", "medium", "Asphalt 9."),
    ("com.amazon.mShop.android.shopping", R, "preinstalled-app", "medium", "Amazon Shopping."), ("com.amazon.kindle", R, "preinstalled-app", "medium", "Kindle."),
])
# ---- Carriers: whole families, all treated carefully ----
for pre, name in [("com.verizon.", "Verizon"), ("com.vzw.", "Verizon"), ("com.att.", "AT&T"), ("com.tmobile.", "T-Mobile"), ("com.sprint.", "Sprint"),
                  ("com.orange.", "Orange"), ("com.vodafone.", "Vodafone"), ("de.telekom.", "Telekom"), ("com.telefonica.", "Telefonica"), ("com.jio.", "Jio"), ("com.myairtelapp", "Airtel")]:
    add("carriers", [(pre, A, "carrier-family", "low", f"{name} apps and services.", {"prefix": True, "tier": "aggressive"})])

# ---- duplicate links: this app does the same job as that one ----
DUPES = {
    "com.samsung.android.messaging": "com.google.android.apps.messaging", "com.samsung.android.dialer": "com.google.android.dialer",
    "com.samsung.android.contacts": "com.google.android.contacts", "com.samsung.android.calendar": "com.google.android.calendar",
    "com.sec.android.app.sbrowser": "com.android.chrome", "com.sec.android.gallery3d": "com.google.android.apps.photos",
    "com.sec.android.app.myfiles": "com.google.android.apps.nbu.files", "com.sec.android.app.clockpackage": "com.google.android.deskclock",
    "com.sec.android.app.popupcalculator": "com.google.android.calculator", "com.samsung.android.app.notes": "com.google.android.keep",
    "com.samsung.android.email.provider": "com.google.android.gm", "com.samsung.android.bixby.agent": "com.google.android.googlequicksearchbox",
    "com.miui.gallery": "com.google.android.apps.photos", "com.miui.notes": "com.google.android.keep", "com.miui.calculator": "com.google.android.calculator",
    "com.android.browser": "com.android.chrome", "com.mi.globalbrowser": "com.android.chrome", "com.miui.player": "com.google.android.apps.youtube.music",
    "com.miui.videoplayer": "com.google.android.videos", "com.mi.android.globalFileexplorer": "com.google.android.apps.nbu.files",
    "com.huawei.browser": "com.android.chrome", "com.huawei.music": "com.google.android.apps.youtube.music", "com.huawei.himovie.overseas": "com.google.android.videos",
    "com.huawei.notepad": "com.google.android.keep", "com.heytap.browser": "com.android.chrome", "com.heytap.music": "com.google.android.apps.youtube.music",
    "com.coloros.note": "com.google.android.keep", "com.oneplus.gallery": "com.google.android.apps.photos", "com.oneplus.filemanager": "com.google.android.apps.nbu.files",
    "com.oneplus.deskclock": "com.google.android.deskclock", "com.coloros.calculator": "com.google.android.calculator", "com.motorola.gallery": "com.google.android.apps.photos",
    "com.sonyericsson.album": "com.google.android.apps.photos", "com.sonyericsson.music": "com.google.android.apps.youtube.music", "com.sonymobile.email": "com.google.android.gm",
    "com.vivo.browser": "com.android.chrome", "com.android.bbkcalculator": "com.google.android.calculator", "com.android.BBKClock": "com.google.android.deskclock", "com.android.notes": "com.google.android.keep",
}

def build(file, e, extra):
    id_, risk, cat, conf, desc = e
    extra = extra or {}
    row = {"id": id_, "risk": risk, "desc": desc, "category": cat}
    if cat in ("carrier-family",): row["category"] = "carrier"
    if cat == "core-system":
        row["confidence"] = "high"; row["impact"] = IMPACT["core-family" if extra.get("prefix") else "core-system"]
    else:
        tier = extra.get("tier") or TIER_BY_CAT.get(row["category"], "balanced")
        if tier == "aggressive" and risk == R: risk = A
        row["risk"] = risk; row["tier"] = tier; row["confidence"] = conf
        row["impact"] = extra.get("impact") or IMPACT.get(cat) or IMPACT.get(row["category"]) or IMPACT["other"]
        if row["category"] in REINSTALL: row["reinstall"] = REINSTALL[row["category"]]
    if extra.get("prefix"): row["prefix"] = True
    return row

def main():
    files = {}
    existing = set()
    for f in glob.glob(f"{DIR}/*.json"):
        files[os.path.basename(f)[:-5]] = json.load(open(f))
        existing |= {r["id"] for r in files[os.path.basename(f)[:-5]]}
    added = 0
    for entry in NEW:
        file, *rest = entry
        extra = rest[5] if len(rest) > 5 else None
        e = tuple(rest[:5])
        if e[0] in existing: continue
        files.setdefault(file, [])
        files[file].append(build(file, e, extra)); existing.add(e[0]); added += 1
    linked = 0
    for rows in files.values():
        for r in rows:
            if r["id"] in DUPES and r.get("duplicate_of") != DUPES[r["id"]]:
                r["duplicate_of"] = DUPES[r["id"]]; linked += 1
    for name, rows in files.items():
        json.dump(rows, open(f"{DIR}/{name}.json", "w"), indent=1); open(f"{DIR}/{name}.json", "a").write("\n")
    print(f"added {added} entries, linked {linked} duplicates, total {sum(len(r) for r in files.values())}")

if __name__ == "__main__":
    sys.exit(main())
