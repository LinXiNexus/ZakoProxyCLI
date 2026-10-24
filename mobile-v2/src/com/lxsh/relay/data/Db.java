package com.lxsh.relay.data;

import android.content.Context;
import android.database.Cursor;
import android.database.sqlite.SQLiteDatabase;
import android.util.Log;
import com.lxsh.relay.core.MigrationUtil;
import java.io.File;

/**
 * Opens the LXSH SQLite store (legacy-compatible: config/sessions/requests tables).
 * The file lives at filesDir/data/lxsh.db; legacy zako.db is migrated on first run.
 */
public final class Db {
    private static final String TAG = "Db";
    private volatile SQLiteDatabase db;

    public static File dataDir(Context ctx) {
        return new File(ctx.getFilesDir(), "data");
    }

    public synchronized SQLiteDatabase open(Context ctx) {
        if (db != null && db.isOpen()) return db;
        MigrationUtil.migrateIfNeeded(ctx);
        File dir = dataDir(ctx);
        // noinspection ResultOfMethodCallIgnored
        dir.mkdirs();
        File file = new File(dir, "lxsh.db");
        if (!file.exists()) {
            Log.i(TAG, "fresh database, will be initialized by setup");
            db = SQLiteDatabase.openOrCreateDatabase(file, null);
            initSchema(db);
            return db;
        }
        db = SQLiteDatabase.openDatabase(file.getAbsolutePath(), null,
                SQLiteDatabase.OPEN_READWRITE | SQLiteDatabase.NO_LOCALIZED_COLLATORS);
        return db;
    }

    private static void initSchema(SQLiteDatabase d) {
        d.execSQL("CREATE TABLE IF NOT EXISTS config (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);");
        d.execSQL("CREATE TABLE IF NOT EXISTS sessions (hash TEXT PRIMARY KEY, expires INTEGER NOT NULL, admin_id TEXT);");
        d.execSQL("CREATE TABLE IF NOT EXISTS requests (id TEXT PRIMARY KEY, timestamp INTEGER NOT NULL, value TEXT NOT NULL);");
        d.execSQL("CREATE INDEX IF NOT EXISTS requests_time ON requests(timestamp);");
        d.execSQL("CREATE TABLE IF NOT EXISTS agent_conversations (id TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);");
        d.execSQL("CREATE INDEX IF NOT EXISTS agent_conversations_time ON agent_conversations(updated_at);");
    }

    /** config table single row JSON value (matches Rust store.read_config). */
    public String configJson() {
        if (db == null || !db.isOpen()) return "{}";
        try (Cursor c = db.rawQuery("SELECT value FROM config WHERE id=1", null)) {
            if (c.moveToFirst()) return c.getString(0);
        } catch (Exception e) {
            Log.e(TAG, "config read failed", e);
        }
        return "{}";
    }

    public void writeConfig(String json) {
        if (db == null || !db.isOpen()) return;
        db.execSQL("INSERT INTO config(id, value) VALUES(1, ?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",
                new Object[]{json});
    }

    public void close() {
        if (db != null) { db.close(); db = null; }
    }
}
