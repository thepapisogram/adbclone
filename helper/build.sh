#!/bin/sh
# Rebuilds helper/dist/helper.jar from source so anyone can check the prebuilt jar.
# Needs: a JDK (javac, jar), the apt packages `dalvik-exchange` (dx) and `android-sdk-platform-23` (android.jar).
set -eu
cd "$(dirname "$0")"
ANDROID_JAR=${ANDROID_JAR:-/usr/lib/android-sdk/platforms/android-23/android.jar}
work=$(mktemp -d)
mkdir -p "$work/classes" dist
javac --release 8 -Xlint:-options -cp "$ANDROID_JAR" -d "$work/classes" src/dev/adbclone/helper/Main.java
dalvik-exchange --dex --min-sdk-version=21 --output="$work/classes.dex" "$work/classes"
# A fixed timestamp keeps the jar byte-for-byte reproducible.
(cd "$work" && jar --create --file "$OLDPWD/dist/helper.jar" --date=2024-01-01T00:00:00Z classes.dex)
sha256sum dist/helper.jar | cut -d' ' -f1 > dist/helper.jar.sha256
rm -rf "$work"
echo "built dist/helper.jar ($(wc -c < dist/helper.jar) bytes, sha256 $(cat dist/helper.jar.sha256))"
