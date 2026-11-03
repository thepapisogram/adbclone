#!/usr/bin/env python3
"""A stand-in for `adb` used by integration tests. Usage: fake_adb.py <dir> [--no-unzip] <adb args...>
<dir> holds the APK fixtures, plus snap1.txt, snap2.txt (process snapshots served in turn) and a counter file."""
import os, shutil, sys

d, args = sys.argv[1], sys.argv[2:]
no_unzip = False
if args and args[0] == "--no-unzip":
    no_unzip, args = True, args[1:]
if args[:1] == ["-s"]:
    args = args[2:]

def fixture(path):
    return os.path.join(d, os.path.basename(path))

def read(name, default=""):
    p = os.path.join(d, name)
    return open(p).read() if os.path.exists(p) else default

if args[:1] in (["install"], ["install-multiple"]):
    flags = [a for a in args[1:] if a.startswith("-")]
    files = [a for a in args[1:] if not a.startswith("-")]
    open(os.path.join(d, "installed.log"), "a").write(args[0] + " " + " ".join(flags) + " :: " + " ".join(os.path.basename(f) for f in files) + "\n")
    print("Performing Streamed Install")
    bad = [f for f in files if "exists" in os.path.basename(f)]
    if bad and "-r" not in flags:
        sys.stderr.write(f"adb: failed to install {bad[0]}: Failure [INSTALL_FAILED_ALREADY_EXISTS: Attempt to re-install without first uninstalling.]\n"); sys.exit(1)
    if any("abi" in os.path.basename(f) for f in files):
        sys.stderr.write("adb: failed to install x.apk: Failure [INSTALL_FAILED_NO_MATCHING_ABIS]\n"); sys.exit(1)
    print("Success")
elif args[:1] == ["push"]:
    shutil.copy(args[1], os.path.join(d, "pushed.jar"))
    print(f"{args[1]}: 1 file pushed.")
elif args[:1] == ["exec-out"] and len(args) > 1 and args[1].startswith("CLASSPATH="):
    words = args[1].split()
    mode = words[words.index("dev.adbclone.helper.Main") + 1]
    wanted = words[words.index("dev.adbclone.helper.Main") + 2:]
    f = os.path.join(d, f"helper_{mode}.txt")
    if not os.path.exists(f):
        sys.stderr.write("app_process: inaccessible or not found\n"); sys.exit(127)
    for l in open(f).read().splitlines():
        if mode == "icons" and wanted and '"p":"' in l and not any(f'"p":"{w}"' in l for w in wanted):
            continue
        print(l)
elif args[:1] == ["version"]:
    print("Android Debug Bridge version 1.0.41\nVersion 34.0.4-10411341\nInstalled as /fake/adb")
elif args[:1] == ["pair"]:
    addr, code = args[1], args[2]
    if code == "123456":
        print(f"Successfully paired to {addr} [guid=adb-FAKE-xyz]")
    elif code == "999999":
        sys.stderr.write("error: protocol fault (couldn't read status): connection reset\n"); sys.exit(1)
    else:
        print("Failed: Wrong password or connection was dropped. Check the pairing code and try again.")
elif args[:1] == ["connect"]:
    addr = args[1]
    if addr.startswith("10.9.9."):
        print(f"cannot connect to {addr}: A connection attempt failed (10060)")
    elif addr.endswith(":1"):
        print(f"failed to connect to '{addr}': Connection refused")
    else:
        seen = read("connected").split()
        if addr in seen:
            print(f"already connected to {addr}")
        else:
            open(os.path.join(d, "connected"), "a").write(addr + "\n")
            print(f"connected to {addr}")
elif args[:1] == ["disconnect"]:
    print(f"disconnected {args[1]}")
elif args[:2] == ["mdns", "services"]:
    sys.stdout.write("List of discovered mdns services\n" + read("mdns.txt"))
elif args[:1] == ["tcpip"]:
    print(f"restarting in TCP mode port: {args[1]}")
elif args[:3] == ["exec-out", "unzip", "-p"]:
    import zipfile
    if no_unzip:
        sys.stderr.write("unzip: not found\n"); sys.exit(127)
    try:
        sys.stdout.buffer.write(zipfile.ZipFile(fixture(args[3])).read(args[4]))
    except KeyError:
        sys.exit(1)
elif args[:1] == ["pull"]:
    shutil.copy(fixture(args[1]), args[2])
    print("1 file pulled")
elif args[:1] == ["shell"]:
    cmd = " ".join(args[1:])
    if cmd.startswith("rm -f "):
        open(os.path.join(d, "removed"), "a").write(cmd.split()[-1] + "\n")
    elif cmd.startswith(("settings ", "cmd appops ", "am get-standby-bucket", "am set-standby-bucket", "pm trim-caches")) or "@@TUNE" in cmd:
        import json
        tp = os.path.join(d, "tunedb.json")
        t = json.load(open(tp)) if os.path.exists(tp) else {"settings": {}, "appops": {}, "standby": {}}
        w = cmd.split()
        def save_t(): json.dump(t, open(tp, "w"))
        if "@@TUNE" in cmd:
            print("@@TUNE")
            for k in ["global.window_animation_scale", "global.low_power", "system.peak_refresh_rate"]:
                print(k, t["settings"].get(k, "null"))
            print("@@FPS")
            for f in t.get("fps", [60.0]): print(f"fps={f}")
        elif w[0] == "settings" and w[1] == "get":
            print(t["settings"].get(f"{w[2]}.{w[3]}", "null"))
        elif w[0] == "settings" and w[1] == "put":
            if t.get("ignore") and w[3] in t["ignore"]: pass  # a maker that silently ignores the write
            else: t["settings"][f"{w[2]}.{w[3]}"] = w[4]; save_t()
        elif w[0] == "settings" and w[1] == "delete":
            t["settings"].pop(f"{w[2]}.{w[3]}", None); save_t(); print("Deleted 1 rows")
        elif w[:3] == ["cmd", "appops", "get"]:
            m = t["appops"].get(f"{w[3]}|{w[4]}")
            print(f"{w[4]}: {m}" if m else "No operations.")
        elif w[:3] == ["cmd", "appops", "set"]:
            if w[5] == "default": t["appops"].pop(f"{w[3]}|{w[4]}", None)
            else: t["appops"][f"{w[3]}|{w[4]}"] = w[5]
            save_t()
        elif w[:2] == ["am", "get-standby-bucket"]:
            print(t["standby"].get(w[2], "10"))
        elif w[:2] == ["am", "set-standby-bucket"]:
            t["standby"][w[2]] = {"active": "10", "working_set": "20", "frequent": "30", "rare": "40", "restricted": "45"}[w[3]]; save_t()
        elif w[:2] == ["pm", "trim-caches"]:
            pass
    elif "@@USAGE" in cmd:
        sys.stdout.write(read("usage.txt"))
    elif cmd.strip() == "dumpsys diskstats":
        sys.stdout.write(read("diskstats.txt"))
    elif "@@HOMEALL" in cmd:
        sys.stdout.write(read("protect.txt"))
    elif "@@PROPS" in cmd:
        sys.stdout.write(read("deviceinfo.txt"))
    elif "@@THERMAL" in cmd:
        sys.stdout.write(read("health.txt"))
    elif cmd.startswith("pm ") or cmd.startswith("cmd package "):
        import json
        db_path = os.path.join(d, "pkgdb.json")
        db = json.load(open(db_path))
        words = cmd.split()
        def save():
            json.dump(db, open(db_path, "w"))
        def target():
            return words[-1]
        if cmd.startswith("pm list packages"):
            flags = set(words[3:])
            for i, (name, p) in enumerate(sorted(db.items())):
                st = p["state"]
                if st == "uninstalled" and "-u" not in flags: continue
                if "-s" in flags and not p["system"]: continue
                if "-3" in flags and p["system"]: continue
                if "-d" in flags and st != "disabled": continue
                if "-e" in flags and st != "enabled": continue
                print(f"package:/system/app/{name}.apk={name} uid:{10000 + i}")
        else:
            name = target()
            if name not in db:
                print("Exception: Package " + name + " doesn't exist"); sys.exit(0)
            if "fail" in name and ("enable" in words or "disable-user" in words):
                print("Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]"); sys.exit(0)
            if words[1] == "path":
                if db[name]["state"] != "uninstalled":
                    for pth in db[name].get("paths", [f"/system/app/{name}.apk"]): print("package:" + pth)
                save(); sys.exit(0)
            if words[1] == "disable-user": db[name]["state"] = "disabled"; print(f"Package {name} new state: disabled-user")
            elif words[1] == "enable": db[name]["state"] = "enabled"; print(f"Package {name} new state: enabled")
            elif words[1] == "uninstall": db[name]["state"] = "uninstalled"; print("Success")
            elif "install-existing" in words: db[name]["state"] = "enabled"; print(f"Package {name} installed for user: 0")
            save()
    elif cmd.startswith("ip -f inet addr show"):
        sys.stdout.write(read("ipaddr.txt", 'Device "wlan0" does not exist.\n'))
    elif cmd.startswith("stat -c"):
        for p in cmd.split("2>/dev/null")[0].split("'")[-1].split():
            f = fixture(p)
            if os.path.exists(f):
                print(os.path.getsize(f), 1700000000, p)
    elif "@@STAT" in cmd:
        counter = os.path.join(d, "counter")
        n = int(open(counter).read()) + 1 if os.path.exists(counter) else 1
        open(counter, "w").write(str(n))
        sys.stdout.write(open(os.path.join(d, f"snap{min(n, 2)}.txt")).read())
    else:
        sys.stderr.write("fake_adb: unsupported shell command\n"); sys.exit(1)
else:
    sys.stderr.write("fake_adb: unsupported command\n"); sys.exit(1)
