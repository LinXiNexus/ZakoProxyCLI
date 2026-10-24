package com.lxsh.relay.ui;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import com.lxsh.relay.R;

import java.io.BufferedReader;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;

/**
 * M2: auth screen — checks /api/bootstrap, then shows an init (first-run)
 * or login form. Stores the session cookie in SharedPreferences.
 */
public final class AuthActivity extends Activity {
    private static final String PREFS = "lxsh_prefs";
    public static final String SESSION_KEY = "session_cookie";

    private TextView title;
    private TextView subtitle;
    private EditText user;
    private EditText pass;
    private Button submit;
    private TextView error;
    private boolean isSetup;
    private final Handler handler = new Handler(Looper.getMainLooper());

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(buildUi());
        checkBootstrap();
    }

    private View buildUi() {
        ScrollView scroll = new ScrollView(this);
        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        int pad = Math.round(getResources().getDisplayMetrics().density * 28);
        box.setPadding(pad, pad * 2, pad, pad);

        title = new TextView(this);
        title.setText("欢迎使用 LXSH");
        title.setTextSize(24);
        title.setTypeface(android.graphics.Typeface.create("sans-serif", android.graphics.Typeface.BOLD));
        title.setTextColor(colorRes(R.color.green));
        box.addView(title);

        subtitle = new TextView(this);
        subtitle.setText("正在检查本地服务…");
        subtitle.setTextSize(12);
        subtitle.setTextColor(colorRes(R.color.text_secondary_light));
        box.addView(subtitle, new LinearLayout.LayoutParams(-1, -2));
        ((LinearLayout.LayoutParams) subtitle.getLayoutParams()).topMargin = dp(8);

        user = input("用户名");
        pass = input("管理员密码");
        pass.setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD);
        box.addView(user);
        box.addView(pass);

        submit = new Button(this);
        submit.setText("登 录");
        submit.setTextSize(15);
        submit.setTextColor(android.graphics.Color.WHITE);
        submit.setBackgroundColor(colorRes(R.color.green));
        submit.setOnClickListener(v -> submit());
        box.addView(submit, new LinearLayout.LayoutParams(-1, -2));
        ((LinearLayout.LayoutParams) submit.getLayoutParams()).topMargin = dp(20);

        error = new TextView(this);
        error.setTextSize(12);
        error.setTextColor(android.graphics.Color.parseColor("#C05B54"));
        error.setVisibility(View.GONE);
        box.addView(error);

        scroll.addView(box);
        return scroll;
    }

    private EditText input(String hint) {
        EditText e = new EditText(this);
        e.setHint(hint);
        e.setTextSize(15);
        e.setTextColor(colorRes(R.color.text_primary_light));
        e.setHintTextColor(colorRes(R.color.text_secondary_light));
        e.setSingleLine(true);
        e.setBackgroundColor(0x66FFFFFF);
        e.setPadding(dp(14), dp(12), dp(14), dp(12));
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-1, -2);
        lp.topMargin = dp(14);
        e.setLayoutParams(lp);
        return e;
    }

    private void checkBootstrap() {
        new Thread(() -> {
            try {
                HttpURLConnection c = (HttpURLConnection)
                        new URL("http://127.0.0.1:1991/api/bootstrap").openConnection();
                c.setConnectTimeout(1500);
                c.setReadTimeout(1500);
                int code = c.getResponseCode();
                String body = code == 200 ? readAll(c.getInputStream()) : "{}";
                c.disconnect();
                isSetup = new org.json.JSONObject(body).optBoolean("setupRequired", false);
                handler.post(() -> {
                    if (isSetup) {
                        title.setText("初始化 LXSH");
                        subtitle.setText("首次运行，请创建管理员账号");
                        submit.setText("创建管理员");
                    } else {
                        title.setText("欢迎回来");
                        subtitle.setText("登录到本地控制台");
                        submit.setText("登 录");
                    }
                });
            } catch (Exception e) {
                handler.post(() -> showError("无法连接本地服务: " + e.getMessage()));
            }
        }).start();
    }

    private void submit() {
        error.setVisibility(View.GONE);
        String u = user.getText().toString().trim();
        String p = pass.getText().toString().trim();
        if (u.isEmpty() || p.isEmpty()) { showError("请填写用户名和密码"); return; }
        submit.setEnabled(false);
        new Thread(() -> {
            try {
                // 密码哈希：沿用 Argon2 的 PHC 格式无法在 Java 侧重算，这里客户端直接传明文，
                // 服务端在 M2.5 引入 BouncyCastle 做 Argon2 校验前暂以 SHA-256 摘要占位。
                String hash = sha256(p);
                org.json.JSONObject body = new org.json.JSONObject();
                body.put("username", u);
                body.put("password_hash", hash);
                String ep = isSetup ? "/api/setup" : "/api/login";
                HttpURLConnection c = (HttpURLConnection)
                        new URL("http://127.0.0.1:1991" + ep).openConnection();
                c.setRequestMethod("POST");
                c.setDoOutput(true);
                c.setRequestProperty("Content-Type", "application/json");
                try (OutputStream os = c.getOutputStream()) {
                    os.write(body.toString().getBytes(StandardCharsets.UTF_8));
                }
                int code = c.getResponseCode();
                String setCookie = c.getHeaderField("Set-Cookie");
                String resp = readAll(code >= 200 && code < 300 ? c.getInputStream() : c.getErrorStream());
                c.disconnect();
                if (code == 200 && setCookie != null && setCookie.contains("=")) {
                    String cookie = setCookie.split(";")[0];
                    getSharedPreferences(PREFS, MODE_PRIVATE).edit().putString(SESSION_KEY, cookie).apply();
                    handler.post(() -> {
                        startActivity(new Intent(this, MainActivity.class));
                        finish();
                    });
                } else {
                    handler.post(() -> { showError(extractError(resp)); submit.setEnabled(true); });
                }
            } catch (Exception e) {
                handler.post(() -> { showError("请求失败: " + e.getMessage()); submit.setEnabled(true); });
            }
        }).start();
    }

    private void showError(String msg) {
        error.setText(msg);
        error.setVisibility(View.VISIBLE);
    }

    private static String extractError(String json) {
        try {
            return new org.json.JSONObject(json).optString("error", "登录失败");
        } catch (Exception e) { return json; }
    }

    private static String sha256(String s) {
        try {
            java.security.MessageDigest md = java.security.MessageDigest.getInstance("SHA-256");
            byte[] h = md.digest(s.getBytes(StandardCharsets.UTF_8));
            StringBuilder sb = new StringBuilder();
            for (byte b : h) sb.append(String.format("%02x", b));
            return sb.toString();
        } catch (Exception e) { return s; }
    }

    private static String readAll(InputStream in) throws Exception {
        if (in == null) return "";
        BufferedReader r = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8));
        StringBuilder sb = new StringBuilder();
        String line;
        while ((line = r.readLine()) != null) sb.append(line);
        return sb.toString();
    }

    private int dp(int v) { return Math.round(getResources().getDisplayMetrics().density * v); }
    private int colorRes(int c) { return getResources().getColor(c); }
}
