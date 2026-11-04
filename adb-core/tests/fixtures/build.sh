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

# Apps with icons. withicon: a plain PNG mipmap in two densities. xmlicon: the manifest icon is an XML drawable
# (adaptive/vector style) and the real pictures are the legacy launcher PNGs next to it.
png() { python3 -c "
import struct,sys,zlib
n=int(sys.argv[1]); raw=b''.join(b'\x00'+bytes([(i*37+j*11+k*5)%256 for i in range(n) for k in range(3)]) for j in range(n))
def ch(t,d): c=struct.pack('>I',len(d))+t+d; return c+struct.pack('>I',zlib.crc32(t+d)&0xffffffff)
sys.stdout.buffer.write(b'\x89PNG\r\n\x1a\n'+ch(b'IHDR',struct.pack('>IIBBBBB',n,n,8,2,0,0,0))+ch(b'IDAT',zlib.compress(raw))+ch(b'IEND',b''))" "$1" > "$2"; }
mkicon() { # name package icon_ref with_xml
  d=$(mktemp -d); mkdir -p "$d/res/values" "$d/res/mipmap-hdpi" "$d/res/mipmap-xxhdpi"
  printf '<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="%s"><application android:label="Icon App" android:icon="%s"/></manifest>' "$2" "$3" > "$d/AndroidManifest.xml"
  printf '<resources><string name="x">x</string></resources>' > "$d/res/values/strings.xml"
  png 6 "$d/res/mipmap-hdpi/ic_launcher.png"; png 12 "$d/res/mipmap-xxhdpi/ic_launcher.png"
  if [ -n "$4" ]; then mkdir -p "$d/res/drawable"; printf '<shape xmlns:android="http://schemas.android.com/apk/res/android"><solid android:color="#ff0000"/></shape>' > "$d/res/drawable/ic_adaptive.xml"; fi
  aapt package -f -M "$d/AndroidManifest.xml" -S "$d/res" -I "$JAR" -F "$1.apk"
  rm -rf "$d"
}
mkicon withicon com.test.withicon '@mipmap/ic_launcher' ''
mkicon xmlicon com.test.xmlicon '@drawable/ic_adaptive' yes
