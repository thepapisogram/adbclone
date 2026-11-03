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
mkx() { # name package application_attrs inner_xml strings
  d=$(mktemp -d); mkdir -p "$d/res/values"
  printf '<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="%s"><application %s>%s</application></manifest>' "$2" "$3" "$4" > "$d/AndroidManifest.xml"
  printf '<resources>%s</resources>' "$5" > "$d/res/values/strings.xml"
  aapt package -f -M "$d/AndroidManifest.xml" -S "$d/res" -I "$JAR" -F "$1.apk"
  rm -rf "$d"
}
LAUNCH='<intent-filter><action android:name="android.intent.action.MAIN"/><category android:name="android.intent.category.LAUNCHER"/></intent-filter>'
# No application label: the name only exists on the launcher activity (a resource, and a literal one).
mkx activityref com.test.activityref '' "<activity android:name=\".Settings\" android:label=\"@string/other\"/><activity android:name=\".Main\" android:label=\"@string/launcher_name\">$LAUNCH</activity>" '<string name="launcher_name">Launcher Label</string><string name="other">Not This One</string>'
mkx activityliteral com.test.activityliteral '' "<activity android:name=\".Main\" android:label=\"Literal Launcher\">$LAUNCH</activity>" '<string name="x">x</string>'
mk literal com.test.literal 'android:label="Literal App"' '<string name="x">x</string>'
mk reference com.test.reference 'android:label="@string/app_name"' '<string name="app_name">Referenced App</string>' '<string name="app_name">Referenzierte App</string>'
mk entities com.test.entities 'android:label="@string/app_name"' '<string name="app_name">Fish &amp; Chips</string>'
mk nolabel com.test.nolabel '' '<string name="x">x</string>'
