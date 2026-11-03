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

if args[:3] == ["exec-out", "unzip", "-p"]:
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
    if cmd.startswith("stat -c"):
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
