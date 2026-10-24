package com.lxsh.relay.data;

import android.content.Context;
import android.database.Cursor;
import android.util.Log;
import com.lxsh.relay.core.Router;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;

/** REST API routes for the embedded console (subset; M1 skeleton). */
public final class Api {
    private static final String TAG = "Api";

    private final Db db;
    private final SessionManager sessions;
    private final Context ctx;

    public Api(Db db, Context ctx) {
        this.db = db;
        this.ctx = ctx;
        this.sessions = new SessionManager(db, ctx);
    }

    public void register(Router r) {
        r.get("/health", this::health);
        r.get("/api/bootstrap", this::bootstrap);
        r.post("/api/setup", this::setup);
        r.post("/api/login", this::login);
        r.post("/api/logout", this::logout);
        r.get("/api/settings", this::settings);
        r.get("/api/stats", this::stats);
        r.get("/api/me", this::me);
    }

    private Router.Response health(Router.Request req) {
        return Router.Response.json(200, "{\"ok\":true}");
    }

    private Router.Response bootstrap(Router.Request req) {
        JSONObject cfg = Json.obj(db.configJson());
        boolean ready = !cfg.optString("username", "").isEmpty();
        return Router.Response.json(200, "{\"setupRequired\":" + (!ready) + "}");
    }

    private Router.Response setup(Router.Request req) {
        try {
            JSONObject input = Json.obj(new String(req.body, StandardCharsets.UTF_8));
            String username = input.optString("username", "").trim();
            String passwordHash = input.optString("password_hash", "").trim();
            if (username.isEmpty() || passwordHash.isEmpty()) {
                return Router.Response.json(400, "{\"error\":\"username and password_hash required\"}");
            }
            JSONObject cfg = Json.obj(db.configJson());
            if (!cfg.optString("username", "").isEmpty() && cfg.optJSONArray("admins") == null) {
                return Router.Response.json(409, "{\"error\":\"already configured\"}");
            }
            JSONObject admin = new JSONObject();
            admin.put("id", java.util.UUID.randomUUID().toString());
            admin.put("username", username);
            admin.put("password_hash", passwordHash);
            admin.put("role", "owner");
            admin.put("created_at", System.currentTimeMillis() / 1000);
            org.json.JSONArray admins = cfg.optJSONArray("admins");
            if (admins == null) admins = new org.json.JSONArray();
            admins.put(admin);
            cfg.put("admins", admins);
            cfg.put("username", username);
            cfg.put("password_hash", passwordHash);
            db.writeConfig(cfg.toString());
            String secret = sessions.createSession(cfg, username, passwordHash, true);
            Router.Response resp = Router.Response.json(200, "{\"ok\":true}");
            if (secret != null) resp.headers.put("Set-Cookie", "lxsh_session=" + secret
                    + "; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400");
            return resp;
        } catch (Exception e) {
            Log.e(TAG, "setup failed", e);
            return Router.Response.json(500, "{\"error\":\"setup failed\"}");
        }
    }

    private Router.Response login(Router.Request req) {
        try {
            JSONObject input = Json.obj(new String(req.body, StandardCharsets.UTF_8));
            String username = input.optString("username", "").trim();
            String hash = input.optString("password_hash", "").trim();
            JSONObject cfg = Json.obj(db.configJson());
            JSONObject admin = SessionManager.findAdmin(cfg, username, hash);
            boolean valid = admin != null || (cfg.optString("username").equals(username) && cfg.optString("password_hash").equals(hash));
            if (!valid) return Router.Response.json(401, "{\"error\":\"invalid credentials\"}");
            String secret = sessions.createSession(cfg, username, hash, true);
            Router.Response resp = Router.Response.json(200, "{\"ok\":true,\"admin\":\"" + username + "\"}");
            if (secret != null) resp.headers.put("Set-Cookie", "lxsh_session=" + secret
                    + "; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400");
            return resp;
        } catch (Exception e) {
            return Router.Response.json(500, "{\"error\":\"login failed\"}");
        }
    }

    private Router.Response logout(Router.Request req) {
        String cookie = req.cookie("lxsh_session");
        if (cookie != null) sessions.logout(cookie);
        Router.Response resp = Router.Response.json(200, "{\"ok\":true}");
        resp.headers.put("Set-Cookie", "lxsh_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
        return resp;
    }

    private boolean authed(Router.Request req) {
        String cookie = req.cookie("lxsh_session");
        return cookie != null && sessions.authenticated(cookie);
    }

    private Router.Response settings(Router.Request req) {
        if (!authed(req)) return Router.Response.json(401, "{\"error\":\"unauthorized\"}");
        JSONObject cfg = Json.obj(db.configJson());
        JSONObject out = new JSONObject();
        try {
            out.put("retention_days", cfg.optJSONObject("settings") != null
                    ? cfg.optJSONObject("settings").optInt("retention_days", 7) : 7);
            out.put("request_timeout_seconds", cfg.optJSONObject("settings") != null
                    ? cfg.optJSONObject("settings").optInt("request_timeout_seconds", 30) : 30);
        } catch (Exception ignored) {}
        return Router.Response.json(200, out.toString());
    }

    private Router.Response me(Router.Request req) {
        if (!authed(req)) return Router.Response.json(401, "{\"error\":\"unauthorized\"}");
        JSONObject cfg = Json.obj(db.configJson());
        JSONObject out = new JSONObject();
        try {
            org.json.JSONArray admins = cfg.optJSONArray("admins");
            out.put("admins", admins == null ? new org.json.JSONArray() : admins);
        } catch (Exception ignored) {}
        return Router.Response.json(200, out.toString());
    }

    private Router.Response stats(Router.Request req) {
        if (!authed(req)) return Router.Response.json(401, "{\"error\":\"unauthorized\"}");
        JSONObject out = new JSONObject();
        try {
            JSONObject summary = new JSONObject();
            int totalKeys = 0, totalAccounts = 0, totalRequests = 0;
            long totalInput = 0, totalOutput = 0;
            JSONObject cfg = Json.obj(db.configJson());
            if (cfg.optJSONArray("keys") != null) totalKeys = cfg.optJSONArray("keys").length();
            if (cfg.optJSONArray("accounts") != null) totalAccounts = cfg.optJSONArray("accounts").length();
            Cursor c = db.open(ctx).rawQuery("SELECT COUNT(*), COALESCE(SUM(CAST(json_extract(value,'$.usage.input_tokens') AS INTEGER)),0), COALESCE(SUM(CAST(json_extract(value,'$.usage.output_tokens') AS INTEGER)),0) FROM requests", null);
            if (c.moveToFirst()) {
                totalRequests = c.getInt(0);
                totalInput = c.getLong(1);
                totalOutput = c.getLong(2);
            }
            c.close();
            summary.put("total_requests", totalRequests);
            summary.put("total_keys", totalKeys);
            summary.put("total_accounts", totalAccounts);
            summary.put("input_tokens", totalInput);
            summary.put("output_tokens", totalOutput);
            summary.put("cyber_count", 0);
            out.put("summary", summary);
            out.put("daily", new org.json.JSONArray());
            out.put("recent", new org.json.JSONArray());
        } catch (Exception e) {
            Log.e(TAG, "stats failed", e);
            return Router.Response.json(500, "{\"error\":\"stats failed\"}");
        }
        return Router.Response.json(200, out.toString());
    }
}
