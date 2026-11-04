package dev.adbclone.helper;

import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.drawable.Drawable;
import android.os.Build;
import android.os.Looper;
import android.util.Base64;
import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.json.JSONArray;
import org.json.JSONObject;

/**
 * Temporary helper run from the computer with:  app_process /system/bin dev.adbclone.helper.Main <mode> [packages...]
 * It runs as the shell user, asks Android's PackageManager about apps, prints lines starting with "ACB1 ",
 * and exits. Nothing is installed and nothing is changed on the phone. Modes:
 *   selftest         what works on this phone
 *   apps             name and details of every package
 *   icons <pkg...>   96x96 PNG icons (base64) for the listed packages
 *   details <pkg>    permissions (with granted state), the app's own description, category
 */
public final class Main {
    private static final int FLAG_INSTALLED = 0x00800000;      // ApplicationInfo.FLAG_INSTALLED
    private static final int GET_UNINSTALLED_PACKAGES = 0x2000;  // also list apps removed for this user
    private static final int GET_DISABLED_COMPONENTS = 0x200;
    private static final int ICON_SIZE = 96;

    private static PrintStream out;

    public static void main(String[] args) {
        try {
            out = new PrintStream(System.out, false, "UTF-8");
        } catch (Exception e) {
            out = System.out;
        }
        String mode = args.length > 0 ? args[0] : "selftest";
        try {
            if ("selftest".equals(mode)) {
                selftest();
            } else if ("apps".equals(mode)) {
                apps();
            } else if ("icons".equals(mode)) {
                Set<String> wanted = new HashSet<String>();
                for (int i = 1; i < args.length; i++) wanted.add(args[i]);
                icons(wanted);
            } else if ("details".equals(mode)) {
                if (args.length < 2) error("details needs a package name"); else details(args[1]);
            } else {
                error("unknown mode: " + mode);
            }
        } catch (Throwable t) {
            error(describe(t));
        }
        line("DONE");
        out.flush();
        // app_process keeps running if non-daemon threads exist; make sure we leave.
        System.exit(0);
    }

    // ---- output ----

    private static void line(String payload) {
        out.print("ACB1 ");
        out.print(payload);
        out.print('\n');
        out.flush();
    }

    private static void error(String message) {
        try {
            line(new JSONObject().put("error", message).toString());
        } catch (Exception ignored) {
            line("{\"error\":\"error\"}");
        }
    }

    private static String describe(Throwable t) {
        Throwable root = t;
        while (root.getCause() != null && root.getCause() != root) root = root.getCause();
        StringBuilder sb = new StringBuilder(root.getClass().getName()).append(": ").append(root.getMessage());
        StackTraceElement[] st = root.getStackTrace();
        for (int i = 0; i < st.length && i < 3; i++) sb.append(" @ ").append(st[i].getClassName()).append('.').append(st[i].getMethodName());
        return sb.toString();
    }

    // ---- android access ----

    private static Context context() throws Exception {
        try {
            Looper.prepareMainLooper();
        } catch (Throwable alreadyPrepared) {
            // fine: ActivityThread only needs a main looper to exist
        }
        Class<?> activityThread = Class.forName("android.app.ActivityThread");
        Object thread = activityThread.getMethod("systemMain").invoke(null);
        return (Context) activityThread.getMethod("getSystemContext").invoke(thread);
    }

    private static List<PackageInfo> packages(PackageManager pm) {
        // Some phones refuse the big flag combination; fall back to simpler ones so we still list something.
        int[] attempts = {GET_UNINSTALLED_PACKAGES | GET_DISABLED_COMPONENTS, GET_UNINSTALLED_PACKAGES, 0};
        for (int flags : attempts) {
            try {
                List<PackageInfo> list = pm.getInstalledPackages(flags);
                if (list != null && !list.isEmpty()) return list;
            } catch (Throwable ignored) {
            }
        }
        return new ArrayList<PackageInfo>();
    }

    // ---- modes ----

    private static void selftest() throws Exception {
        JSONObject o = new JSONObject();
        o.put("sdk", Build.VERSION.SDK_INT);
        o.put("release", Build.VERSION.RELEASE);
        o.put("model", Build.MODEL);
        o.put("uid", android.os.Process.myUid());
        JSONArray problems = new JSONArray();
        try {
            Context c = context();
            o.put("context", c != null);
            PackageManager pm = c.getPackageManager();
            List<PackageInfo> list = packages(pm);
            o.put("packages", list.size());
            if (!list.isEmpty()) {
                PackageInfo first = list.get(0);
                try {
                    CharSequence label = first.applicationInfo.loadLabel(pm);
                    o.put("sampleLabel", String.valueOf(label));
                } catch (Throwable t) {
                    problems.put("label: " + describe(t));
                }
                try {
                    Drawable d = first.applicationInfo.loadIcon(pm);
                    o.put("sampleIcon", d != null);
                    String png = encode(d);
                    o.put("iconEncoded", png != null && png.length() > 100);
                } catch (Throwable t) {
                    problems.put("icon: " + describe(t));
                }
            }
        } catch (Throwable t) {
            o.put("context", false);
            problems.put("context: " + describe(t));
        }
        o.put("problems", problems);
        line(o.toString());
    }

    private static void apps() throws Exception {
        PackageManager pm = context().getPackageManager();
        for (PackageInfo pi : packages(pm)) {
            try {
                line(describeApp(pm, pi).toString());
            } catch (Throwable t) {
                // One broken package must not hide the others.
                error(pi.packageName + ": " + describe(t));
            }
        }
    }

    private static JSONObject describeApp(PackageManager pm, PackageInfo pi) throws Exception {
        ApplicationInfo ai = pi.applicationInfo;
        JSONObject o = new JSONObject();
        o.put("p", pi.packageName);
        String label = "";
        try {
            CharSequence cs = ai.loadLabel(pm);
            if (cs != null) label = cs.toString().trim();
        } catch (Throwable ignored) {
        }
        o.put("n", label);
        o.put("s", (ai.flags & ApplicationInfo.FLAG_SYSTEM) != 0 ? 1 : 0);
        o.put("u", (ai.flags & ApplicationInfo.FLAG_UPDATED_SYSTEM_APP) != 0 ? 1 : 0);
        o.put("e", ai.enabled ? 1 : 0);
        o.put("in", (ai.flags & FLAG_INSTALLED) != 0 ? 1 : 0);
        o.put("v", pi.versionName == null ? "" : pi.versionName);
        o.put("vc", pi.versionCode);
        o.put("fi", pi.firstInstallTime);
        o.put("lu", pi.lastUpdateTime);
        o.put("t", ai.targetSdkVersion);
        try {
            String installer = pm.getInstallerPackageName(pi.packageName);
            o.put("i", installer == null ? "" : installer);
        } catch (Throwable ignored) {
            o.put("i", "");
        }
        try {
            o.put("l", pm.getLaunchIntentForPackage(pi.packageName) != null ? 1 : 0);
        } catch (Throwable ignored) {
            o.put("l", 0);
        }
        return o;
    }

    private static void icons(Set<String> wanted) throws Exception {
        PackageManager pm = context().getPackageManager();
        for (PackageInfo pi : packages(pm)) {
            if (!wanted.isEmpty() && !wanted.contains(pi.packageName)) continue;
            try {
                Drawable d;
                try {
                    d = pi.applicationInfo.loadIcon(pm);
                } catch (Throwable first) {
                    d = pm.getApplicationIcon(pi.packageName);
                }
                String png = encode(d);
                if (png != null) line(new JSONObject().put("p", pi.packageName).put("png", png).toString());
            } catch (Throwable t) {
                error(pi.packageName + ": icon: " + describe(t));
            }
        }
    }

    private static void details(String pkg) throws Exception {
        PackageManager pm = context().getPackageManager();
        int flags = 0x1000 /* GET_PERMISSIONS */ | GET_UNINSTALLED_PACKAGES;
        PackageInfo pi;
        try {
            pi = pm.getPackageInfo(pkg, flags);
        } catch (Throwable t) {
            pi = pm.getPackageInfo(pkg, 0x1000);
        }
        JSONObject o = new JSONObject();
        o.put("p", pkg);
        JSONArray perms = new JSONArray();
        if (pi.requestedPermissions != null) {
            for (int i = 0; i < pi.requestedPermissions.length; i++) {
                int granted = 0;
                if (pi.requestedPermissionsFlags != null && i < pi.requestedPermissionsFlags.length) {
                    granted = (pi.requestedPermissionsFlags[i] & 2 /* REQUESTED_PERMISSION_GRANTED */) != 0 ? 1 : 0;
                }
                perms.put(new JSONObject().put("n", pi.requestedPermissions[i]).put("g", granted));
            }
        }
        o.put("perms", perms);
        ApplicationInfo ai = pi.applicationInfo;
        if (ai != null) {
            try {
                CharSequence d = ai.loadDescription(pm);
                o.put("d", d == null ? "" : d.toString().trim());
            } catch (Throwable ignored) {
                o.put("d", "");
            }
            try {
                o.put("c", ApplicationInfo.class.getField("category").getInt(ai)); // API 26+, so read by name
            } catch (Throwable ignored) {
                o.put("c", -1);
            }
            o.put("src", ai.sourceDir == null ? "" : ai.sourceDir);
        }
        line(o.toString());
    }

    private static String encode(Drawable d) throws Exception {
        if (d == null) return null;
        Bitmap bmp = Bitmap.createBitmap(ICON_SIZE, ICON_SIZE, Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(bmp);
        d.setBounds(0, 0, ICON_SIZE, ICON_SIZE);
        d.draw(canvas);
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        bmp.compress(Bitmap.CompressFormat.PNG, 100, bytes);
        bmp.recycle();
        return Base64.encodeToString(bytes.toByteArray(), Base64.NO_WRAP);
    }
}
