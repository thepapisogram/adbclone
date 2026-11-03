#!/usr/bin/env python3
"""One-time upgrade of src-tauri/data/ratings/*.json to the v2 format (tier, category, impact, confidence, reinstall).
Rules are checked in order; per-id overrides win. Where a rule would make an entry look *safer* than the old
rating, the old rating is kept: this migration may make advice more careful, never less."""
import glob, json, re, sys

DIR = "src-tauri/data/ratings"

def has(i, *parts): return any(p in i for p in parts)

# ---- explicit id sets ----
ADS = ("analytics", "telemetry", "systemadsolution", "msa.global", "opos.ads", "diagmon", "dqagent", "daagent", "knox.analytics", "gnssdebug", "mdmlsample", "mtklogger", "networkdiagnostic", "catcherpatch", "contentcatcher", "google.android.feedback", "bugreport")
PARTNER_STUB = {"com.facebook.appmanager", "com.facebook.services", "com.facebook.system", "com.netflix.partner.activation", "com.amazon.appmanager"}
HELP = {"com.samsung.android.app.tips", "com.samsung.android.voc", "com.motorola.genie", "com.motorola.help", "com.motorola.motocare", "com.miui.miservice", "com.android.protips", "com.xiaomi.glgm", "com.xiaomi.gamecenter.sdk.service", "com.nearme.gamecenter", "com.miui.hybrid", "com.huawei.fastapp", "com.oneplus.membership", "com.samsung.android.kidsinstaller", "com.samsung.android.stickercenter"}
DISCONTINUED = {"com.google.android.music", "com.google.android.talk", "com.google.android.apps.cloudprint", "com.google.android.street", "com.google.android.apps.podcasts", "com.google.android.printservice.recommendation"}
AOSP_EXTRA = {"com.android.egg", "com.android.dreams.basic", "com.android.dreams.phototable", "com.android.noisefield", "com.android.magicsmoke", "com.android.galaxy4", "com.android.phasebeam", "com.android.musicvis", "com.android.traceur", "com.android.dynsystem", "com.android.htmlviewer", "com.android.bookmarkprovider", "com.android.providers.partnerbookmarks", "com.android.providers.tv", "com.android.bluetoothmidiservice", "com.android.wallpaper.livepicker"}
ASSISTANT = ("bixby", "googleassistant", "visionintelligence", "voiceassist", "assistantscreen", "hotwordenrollment", "apps.spage")
STORE = {"com.sec.android.app.samsungapps", "com.samsung.android.app.updatecenter", "com.xiaomi.mipicks", "com.xiaomi.discover", "com.xiaomi.market", "com.huawei.appmarket", "com.hihonor.appmarket", "com.heytap.market", "com.samsung.android.themestore", "com.android.vending"}
CLOUD = ("cloud", "account", "hwid", "scloud", "mobileservice", "samsungpass", "authfw", "honor.id", "hihonor.id")
PAYMENTS = ("spay", "mipay", "xiaomi.payment", "huawei.wallet", "walletnfcrel")
CORE_APP = {"com.android.dialer", "com.google.android.dialer", "com.samsung.android.dialer", "com.android.contacts", "com.google.android.contacts", "com.samsung.android.contacts", "com.android.mms", "com.google.android.apps.messaging", "com.samsung.android.messaging", "com.android.camera2", "com.sec.android.app.camera", "com.oneplus.camera", "com.motorola.camera3", "com.android.inputmethod.latin", "com.google.android.inputmethod.latin", "com.samsung.android.keyboard", "com.sec.android.inputmethod", "com.samsung.android.honeyboard", "com.android.chrome", "com.sec.android.app.sbrowser", "com.google.android.googlequicksearchbox", "com.android.deskclock", "com.sec.android.app.clockpackage", "com.oneplus.deskclock", "com.sec.android.gallery3d", "com.miui.gallery", "com.android.gallery3d", "com.oneplus.gallery", "com.motorola.gallery", "com.sec.android.app.myfiles", "com.mi.android.globalFileexplorer", "com.oneplus.filemanager", "com.android.documentsui", "com.google.android.documentsui"}
GAME = ("game", "candycrush")
SYSTEM_SVC = ("systemmanager", "securitycenter", "securitycore", "securityadd", "powerkeeper", "safecenter", "phonemanager", "oplus.battery", "samsung.android.lool", "sm.devicesecurity", "knox.containercore", "miui.daemon", "joyose", "xmsf", "rubin", "routines", "game.gos", "fmm", "finddevice", "qualcomm", "mediatek", "mdx", "easysetup", "smartmirroring", "sharelive", "cellbroadcast", "emergency", "carrierdefaultapp", "ext.services", "ext.shared", "partnersetup", "onetimeinitializer", "configupdater", "setupwizard", "syncadapters", "gms.location.history", "gms.policy_sidecar", "safetycore", "apps.restore", "apps.turbo", "euicc", "carriersetup", "work.oobconfig", "carrierservices", "tts", "marvin.talkback", "xperiaservices", "sonymobile.entrance", "motorola.setup", "ccc.", "dolby", "faceunlock", "unifiedwfc", "odm", "opbackup", "opwlb", "oplus.battery", "providers.downloads.ui", "providers.calendar", "providers.userdictionary", "certinstaller", "managedprovisioning", "proxyhandler", "android.se", "statementservice", "storagemanager", "settings.intelligence", "vpndialogs", "companiondevicemanager", "android.ons", "sdksandbox", "healthconnect", "ondevicepersonalization", "rkpdapp", "simappdialog", "backupconfirm", "localtransport", "sharedstoragebackup", "pacprocessor", "android.stk", "wallpaperbackup", "wallpapercropper", "android.wallpaper", "antispam", "sync", "huawei.android.hwouc", "miui.cleaner", "miui.powerkeeper", "engineermode", "mediatek.omacp", "lppe", "simprocessor", "att.iqi", "verizon.obdm", "tmobile.simlock", "coloros.safecenter")
BACKGROUND = {"com.samsung.android.beaconmanager", "com.samsung.android.ipsgeofence"}
SYSTEM_EXTRA = {"com.google.android.soundpicker", "com.sec.android.soagent", "com.samsung.android.svcagent", "com.mediatek.batterywarning", "com.miui.miwallpaper", "com.xiaomi.simactivate.service", "com.qti.qualcomm.deviceinfo", "com.samsung.android.bbc.bbcagent", "com.sprint.ms.smf.services", "com.google.android.apps.pixelmigrate"}
VENDOR_EXTRA = {"com.motorola.actions", "com.samsung.android.app.aodservice", "com.samsung.android.privateshare", "com.samsung.android.app.smartcapture", "com.miui.mediaeditor", "com.miui.yellowpage", "com.android.apps.tag", "com.google.android.apps.tachyon", "com.google.android.videos"}
CARRIER_EXTRA = {"com.lookout", "com.coremobility.app.vnotes"}
ASSISTANT_EXTRA = {"com.android.hotwordenrollment.okgoogle", "com.android.hotwordenrollment.xgoogle"}
CORE_EXTRA = {"com.miui.packageinstaller", "com.touchtype.swiftkey"}
CARRIER_FILE = "carriers"
PREINSTALLED_3P = ("facebook", "instagram", "linkedin", "amazon", "audible", "netflix", "spotify", "skype", "flipboard", "microsoft", "touchtype")

IMPACT = {
    "ads-telemetry": "Stops usage reports, ads or diagnostics. Nothing you use directly.",
    "partner-stub": "A helper for a preinstalled partner app. Nothing else depends on it; the app itself keeps working or can be reinstalled.",
    "help-promo": "A tips, help or promotional app. Nothing else depends on it.",
    "game-hub": "You lose the game launcher or its overlay. Games still run.",
    "discontinued": "The maker has stopped developing this app.",
    "aosp-extra": "A leftover extra such as a screensaver, live wallpaper or test tool. Nothing you use depends on it.",
    "assistant": "The voice assistant and its wake word stop working.",
    "vendor-app": "You lose this app. Make sure you have another one for the same job first.",
    "google-app": "You lose this Google app. You can reinstall it from the Play Store.",
    "preinstalled-app": "You lose this app. You can reinstall it from the store.",
    "store": "Installs and updates from this store stop. Apps you already have keep working.",
    "carrier": "Account, billing or support features from your carrier may stop. Calls and data keep working.",
    "cloud-sync": "Sign-in, backup and sync with the maker's cloud stop. Other apps from the maker may need it.",
    "payments": "Payments and wallet features stop working.",
    "background-service": "Background helpers for location or nearby-device features. Features that need them may stop.",
    "system-service": "Features of the maker's system (battery care, security scan, cleaner, sync, setup) may stop working.",
    "core-app": "One of the phone's main apps (calls, messages, camera, keyboard, browser, files, clock or home screen). Make sure another one is set up first.",
    "core-system": "Can stop the phone from working properly. Never remove.",
    "other": "Something you use may stop working. Check what it is first.",
}
ID_IMPACT = {
    "com.android.deskclock": "Alarms and timers may stop working.", "com.sec.android.app.clockpackage": "Alarms and timers may stop working.", "com.oneplus.deskclock": "Alarms and timers may stop working.",
    "com.android.cellbroadcastreceiver": "Emergency alerts (severe weather, safety) stop arriving.", "com.android.cellbroadcastservice": "Emergency alerts (severe weather, safety) stop arriving.", "com.google.android.cellbroadcastreceiver": "Emergency alerts (severe weather, safety) stop arriving.",
    "com.android.emergency": "Emergency information on the lock screen stops working.", "com.android.vending": "You can no longer install or update apps from the Play Store.",
    "com.android.documentsui": "File pickers (choosing a photo or file in an app) may stop working.", "com.google.android.documentsui": "File pickers (choosing a photo or file in an app) may stop working.", "com.miui.packageinstaller": "Installing apps from files may stop working.", "com.touchtype.swiftkey": "The SwiftKey keyboard stops working; make sure another keyboard is turned on first.",
    "com.google.android.inputmethod.latin": "The Google keyboard stops working; make sure another keyboard is turned on first.", "com.google.android.googlequicksearchbox": "Google search, the search bar and Google Assistant stop working.",
    "com.android.chrome": "Chrome stops working; make sure you have another browser.", "com.google.android.apps.maps": "Google Maps stops working. You can reinstall it from the Play Store.",
    "com.google.android.apps.photos": "Photo backup and the Photos app stop working. Make sure your photos are safe first.", "com.google.android.apps.authenticator2": "Your 2-step login codes may stop working. Move them to another app first.",
    "com.google.android.tts": "Spoken text (navigation voice, screen readers) stops working.", "com.google.android.marvin.talkback": "The screen reader for people with low vision stops working.",
    "com.samsung.android.fmm": "You can no longer locate this phone remotely.", "com.xiaomi.finddevice": "You can no longer locate this phone remotely.",
}
HIGH_CONF = {"com.miui.analytics", "com.miui.msa.global", "com.miui.systemAdSolution"} | set(PARTNER_STUB) | set(HELP) | set(AOSP_EXTRA) | set(DISCONTINUED) | {"com.samsung.android.bixby.agent", "com.samsung.android.bixby.service", "com.samsung.android.bixby.wakeup", "com.samsung.android.app.spage", "com.samsung.android.visionintelligence", "com.google.android.youtube", "com.google.android.apps.youtube.music", "com.google.android.apps.docs", "com.google.android.keep", "com.google.android.gm", "com.google.android.calendar", "com.google.android.apps.maps", "com.android.vending", "com.android.chrome", "com.google.android.googlequicksearchbox", "com.microsoft.office.officehubrow", "com.facebook.katana", "com.facebook.orca", "com.instagram.android", "com.netflix.mediaclient", "com.spotify.music"}
TIER_BY_CAT = {"ads-telemetry": "safe", "partner-stub": "safe", "help-promo": "safe", "discontinued": "safe", "aosp-extra": "safe", "game-hub": "balanced", "assistant": "balanced", "vendor-app": "balanced", "google-app": "balanced", "preinstalled-app": "balanced", "store": "balanced", "carrier": "balanced", "other": "balanced", "background-service": "balanced", "cloud-sync": "aggressive", "payments": "aggressive", "system-service": "aggressive", "core-app": "aggressive"}
TIER_OVERRIDE = {"com.android.vending": "aggressive"}
REINSTALL = {"google-app": "store", "preinstalled-app": "store", "discontinued": "store", "game-hub": "store"}

def classify(e, file):
    i, risk = e["id"], e["risk"]
    if risk == "unsafe": return "core-system"
    if i in PARTNER_STUB: return "partner-stub"
    if i in CORE_APP or i in CORE_EXTRA: return "core-app"
    if i in CARRIER_EXTRA: return "carrier"
    if i in ASSISTANT_EXTRA: return "assistant"
    if i in BACKGROUND: return "background-service"
    if i in SYSTEM_EXTRA: return "system-service"
    if i in VENDOR_EXTRA: return "vendor-app"
    if i in HELP: return "help-promo"
    if i in DISCONTINUED: return "discontinued"
    if i in AOSP_EXTRA: return "aosp-extra"
    if i in CORE_APP: return "core-app"
    if i in STORE: return "store"
    if has(i, *PAYMENTS): return "payments"
    if has(i, *ASSISTANT): return "assistant"
    if has(i.lower(), *ADS): return "ads-telemetry"
    if has(i, "gamehome", "gametools", "gamespace", "oplus.games", "game.gamehome") or (has(i, *GAME) and "gos" not in i and "gamecenter" not in i): return "game-hub"
    if has(i, *CLOUD): return "cloud-sync"
    if has(i, *SYSTEM_SVC): return "system-service"
    if file == CARRIER_FILE: return "carrier"
    if any(i.startswith("com." + p) or ("." + p + ".") in i or i.startswith(p) for p in PREINSTALLED_3P) and not i.startswith("com.google"): return "preinstalled-app"
    if i.startswith("com.google."): return "google-app"
    if risk == "recommended": return "vendor-app"
    return "other"

def main():
    out_counts = {}
    for f in sorted(glob.glob(f"{DIR}/*.json")):
        name = f.split("/")[-1][:-5]
        rows = json.load(open(f))
        new = []
        for e in rows:
            if "tier" in e or "category" in e:      # already migrated: leave as is
                new.append(e); continue
            cat = classify(e, name)
            tier = (TIER_OVERRIDE.get(e["id"]) or TIER_BY_CAT[cat]) if cat != "core-system" else None
            risk = e["risk"]
            # Never make advice looser: a more demanding tier needs at least the matching risk.
            if tier == "aggressive" and risk == "recommended": risk = "advanced"
            if tier in ("balanced", "aggressive") and risk == "unsafe": tier = None
            conf = "high" if e["id"] in HIGH_CONF else ("high" if cat == "core-system" else ("low" if cat == "system-service" and any(x in e["id"] for x in ("qualcomm", "mediatek")) else "medium"))
            # A safe tier asks for high confidence to be pre-ticked; anything unverified stays out of Safe.
            r = dict(e); r["risk"] = risk; r["category"] = cat
            if tier: r["tier"] = tier
            r["confidence"] = conf
            r["impact"] = ID_IMPACT.get(e["id"], IMPACT[cat])
            if cat in REINSTALL: r["reinstall"] = REINSTALL[cat]
            new.append(r)
            out_counts[(cat, tier, conf)] = out_counts.get((cat, tier, conf), 0) + 1
        json.dump(new, open(f, "w"), indent=1); open(f, "a").write("\n")
    for k, v in sorted(out_counts.items(), key=lambda kv: -kv[1]): print(f"{v:4d}  {k}")

if __name__ == "__main__":
    sys.exit(main())
