#!/bin/sh
# Regenerates the tiny test APKs (needs `aapt` and android-sdk-platform-23). Unsigned; for parser tests only.
set -e
JAR=/usr/lib/android-sdk/platforms/android-23/android.jar
cd "$(dirname "$0")"
mk() { # name package label_attr strings(default) strings_de
  d=$(mktemp -d); mkdir -p "$d/res/values"
  printf '<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="%s"><application %s/></manifest>' "$2" "$3" > "$d/AndroidManifest.xml"
  printf '<resources>%s</resources>' "$4" > "$d/res/values/strings.xml"
  if [ -n "$5" ]; then mkdir -p "$d/res/values-de"; printf '<resources>%s</resources>' "$5" > "$d/res/values-de/strings.xml"; fi
  aapt package -f -M "$d/AndroidManifest.xml" -S "$d/res" -I "$JAR" -F "$1.apk"
  rm -rf "$d"
}
mk literal com.test.literal 'android:label="Literal App"' '<string name="x">x</string>'
mk reference com.test.reference 'android:label="@string/app_name"' '<string name="app_name">Referenced App</string>' '<string name="app_name">Referenzierte App</string>'
mk entities com.test.entities 'android:label="@string/app_name"' '<string name="app_name">Fish &amp; Chips</string>'
mk nolabel com.test.nolabel '' '<string name="x">x</string>'
