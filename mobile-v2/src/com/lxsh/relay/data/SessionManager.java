package com.lxsh.relay.data;

import android.content.Context;
import android.database.Cursor;
import android.database.sqlite.SQLiteDatabase;
import android.util.Base64;
import android.util.Log;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Session cookie management, compatible with the legacy sessions table
 *  (hash = sha256 of the raw session secret). */
public final class SessionManager {
    private static final String TAG = "Session";
    private static final long TTL_MS = 86400_000L; // 24h

    private final Db db;
    private final Context ctx;

    public SessionManager(Db db, Context ctx) {
        this.db = db;
        this.ctx = ctx;
    }

    public static String digest(String raw) {
        try {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            byte[] h = md.digest(raw.getBytes(java.nio.charset.StandardCharsets.UTF_8));
            StringBuilder sb = new StringBuilder();
            for (byte b : h) sb.append(String.format("%02x", b));
            return sb.toString();
        } catch (Exception e) {
            return raw;
        }
    }

    public String createSession(JSONObject config, String username, String passwordHash, boolean valid) {
        if (!valid) return null;
        try {
            String secret = UUID.randomUUID().toString() + "-" + System.nanoTime();
            JSONArray admins = config.optJSONArray("admins");
            String adminId = null;
            if (admins != null) {
                for (int i = 0; i < admins.length(); i++) {
                    JSONObject a = admins.optJSONObject(i);
                    if (a != null && username.equals(a.optString("username")) && passwordHash.equals(a.optString("password_hash"))) {
                        adminId = a.optString("id");
                        break;
                    }
                }
            }
            SQLiteDatabase d = db.open(ctx);
            d.execSQL("INSERT INTO sessions(hash, expires, admin_id) VALUES(?, ?, ?)",
                    new Object[]{digest(secret), System.currentTimeMillis() + TTL_MS, adminId});
            return secret;
        } catch (Exception e) {
            Log.e(TAG, "createSession failed", e);
            return null;
        }
    }

    public boolean authenticated(String secret) {
        if (secret == null || secret.isEmpty()) return false;
        try {
            SQLiteDatabase d = db.open(ctx);
            try (Cursor c = d.rawQuery("SELECT 1 FROM sessions WHERE hash=? AND expires>?",
                    new String[]{digest(secret), String.valueOf(System.currentTimeMillis())})) {
                return c.moveToFirst();
            }
        } catch (Exception e) {
            return false;
        }
    }

    public void logout(String secret) {
        if (secret == null) return;
        try {
            db.open(ctx).execSQL("DELETE FROM sessions WHERE hash=?", new Object[]{digest(secret)});
        } catch (Exception ignored) {}
    }

    /** @return the admin object matching username+hash, or null. */
    public static JSONObject findAdmin(JSONObject config, String username, String hash) {
        JSONArray admins = config.optJSONArray("admins");
        if (admins != null) {
            for (int i = 0; i < admins.length(); i++) {
                JSONObject a = admins.optJSONObject(i);
                if (a != null && username.equals(a.optString("username")) && hash.equals(a.optString("password_hash"))) {
                    return a;
                }
            }
        }
        return null;
    }
}
