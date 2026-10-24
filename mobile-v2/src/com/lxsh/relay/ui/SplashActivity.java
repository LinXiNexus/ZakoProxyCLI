package com.lxsh.relay.ui;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.view.Gravity;
import android.view.View;
import android.widget.FrameLayout;
import android.widget.TextView;
import com.lxsh.relay.R;
import com.lxsh.relay.core.MigrationUtil;
import com.lxsh.relay.core.ProxyService;

/** Splash: migrate legacy data (if any), start the embedded server, then open MainActivity. */
public final class SplashActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(getColor(R.color.bg_light));
        TextView brand = new TextView(this);
        brand.setText("LXSH");
        brand.setTextSize(40);
        brand.setTypeface(android.graphics.Typeface.create("sans-serif", android.graphics.Typeface.BOLD));
        brand.setTextColor(getColor(R.color.green));
        FrameLayout.LayoutParams lp = new FrameLayout.LayoutParams(-2, -2, Gravity.CENTER);
        root.addView(brand, lp);
        setContentView(root);

        // 后台迁移 + 启动服务
        new Thread(() -> {
            MigrationUtil.migrateIfNeeded(this);
            new Handler(getMainLooper()).post(() -> {
                startService(new Intent(this, ProxyService.class));
                new Handler(getMainLooper()).postDelayed(() -> {
                    boolean authed;
                    try {
                        authed = new java.io.File(getFilesDir(), "data/lxsh.db").exists()
                                && getSharedPreferences("lxsh_prefs", MODE_PRIVATE).getString("session_cookie", "").length() > 0;
                    } catch (Exception e) {
                        authed = false;
                    }
                    Intent go = new Intent(this, authed ? MainActivity.class : AuthActivity.class);
                    startActivity(go);
                    finish();
                }, 600);
            });
        }).start();
    }
}
