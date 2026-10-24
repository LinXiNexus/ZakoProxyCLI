package com.lxsh.relay.core;

import android.content.Context;
import android.util.Log;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.OutputStream;

/**
 * One-shot migration from the legacy app (com.zako.relay) into com.lxsh.relay.
 * Copies the legacy data directory via root (su cp) when available, renaming
 * zako.db -> lxsh.db. Idempotent: skips when target db already exists.
 */
public final class MigrationUtil {
    private static final String TAG = "Migration";
    private static final String OLD_BASE = "/data/data/com.zako.relay/files";
    public static final String OLD_DB = "zako.db";

    private MigrationUtil() {}

    public static boolean hasMigrated(Context ctx) {
        return new File(new File(ctx.getFilesDir(), "data"), "lxsh.db").exists();
    }

    /** @return true if legacy data exists and was imported (or already present). */
    public static synchronized boolean migrateIfNeeded(Context ctx) {
        if (hasMigrated(ctx)) return true;
        File dataDir = new File(ctx.getFilesDir(), "data");
        // noinspection ResultOfMethodCallIgnored
        dataDir.mkdirs();
        File oldDb = new File(OLD_BASE + "/data/" + OLD_DB);
        if (!oldDb.exists()) {
            Log.i(TAG, "no legacy data found, fresh start");
            return false;
        }
        // Prefer plain copy when the legacy files are readable (same uid unlikely).
        try {
            if (oldDb.canRead()) {
                copyFile(oldDb, new File(dataDir, "lxsh.db"));
                copyIfExists(new File(OLD_BASE + "/data/master.key"), new File(dataDir, "master.key"));
                Log.i(TAG, "legacy db copied directly");
                return true;
            }
        } catch (Exception ignored) {}
        // Root path: Android toybox has no `stat -c`, so chown to our own uid directly.
        String uid = String.valueOf(android.os.Process.myUid());
        if (runRoot("cp " + OLD_BASE + "/data/" + OLD_DB + " " + dataDir.getAbsolutePath() + "/lxsh.db"
                + " && cp " + OLD_BASE + "/data/master.key " + dataDir.getAbsolutePath() + "/master.key"
                + " && chown " + uid + ":" + uid + " " + dataDir.getAbsolutePath() + "/lxsh.db " + dataDir.getAbsolutePath() + "/master.key"
                + " && chmod 600 " + dataDir.getAbsolutePath() + "/lxsh.db " + dataDir.getAbsolutePath() + "/master.key")) {
            Log.i(TAG, "legacy db imported via root");
            return true;
        }
        Log.w(TAG, "migration failed (no root / no read access)");
        return false;
    }

    private static void copyIfExists(File src, File dst) throws Exception {
        if (src.exists()) copyFile(src, dst);
    }

    private static void copyFile(File src, File dst) throws Exception {
        try (FileInputStream in = new FileInputStream(src); OutputStream out = new FileOutputStream(dst)) {
            byte[] buf = new byte[8192];
            int n;
            while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
        }
    }

    static boolean runRoot(String cmd) {
        Process p = null;
        try {
            p = Runtime.getRuntime().exec(new String[]{"su", "-c", cmd});
            int code = p.waitFor();
            return code == 0;
        } catch (Throwable t) {
            return false;
        } finally {
            if (p != null) p.destroy();
        }
    }
}
