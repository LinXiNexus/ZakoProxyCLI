package com.lxsh.relay.ui;

import android.app.Activity;
import android.app.Fragment;
import android.os.Bundle;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.FrameLayout;
import android.widget.ImageButton;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import com.lxsh.relay.R;

/** Native shell with a glass sidebar + content area (M1 skeleton pages). */
public final class MainActivity extends Activity {
    private FrameLayout content;
    private LinearLayout drawer;
    private View scrim;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(buildUi());
        showPage(0);
    }

    private View buildUi() {
        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(colorRes(R.color.bg_light));

        content = new FrameLayout(this);
        root.addView(content, new FrameLayout.LayoutParams(-1, -1));

        scrim = new View(this);
        scrim.setBackgroundColor(0x66000000);
        scrim.setAlpha(0f);
        scrim.setOnClickListener(v -> closeDrawer());
        root.addView(scrim, new FrameLayout.LayoutParams(-1, -1));

        drawer = buildDrawer();
        root.addView(drawer, new FrameLayout.LayoutParams(dp(256), -1, Gravity.LEFT));
        return root;
    }

    private LinearLayout buildDrawer() {
        LinearLayout side = new LinearLayout(this);
        side.setOrientation(LinearLayout.VERTICAL);
        side.setBackgroundColor(colorRes(R.color.panel_light));
        side.setPadding(dp(16), dp(24), dp(16), dp(16));
        translate(-dp(300));

        TextView brand = new TextView(this);
        brand.setText("LXSH");
        brand.setTextSize(24);
        brand.setTypeface(android.graphics.Typeface.create("sans-serif", android.graphics.Typeface.BOLD));
        brand.setTextColor(colorRes(R.color.green));
        side.addView(brand, new LinearLayout.LayoutParams(-1, -2));

        TextView ver = new TextView(this);
        ver.setText("v2.0.0 · 原生重构");
        ver.setTextSize(10);
        ver.setTextColor(colorRes(R.color.text_secondary_light));
        ver.setPadding(0, 4, 0, 24);
        side.addView(ver);

        String[] items = {"仪表盘", "对话", "临时按键区", "终端", "设置"};
        for (int i = 0; i < items.length; i++) {
            side.addView(navItem(i, items[i]));
        }
        return side;
    }

    private TextView navItem(int index, String label) {
        TextView t = new TextView(this);
        t.setText(label);
        t.setTextSize(15);
        t.setTextColor(colorRes(R.color.text_primary_light));
        t.setGravity(Gravity.CENTER_VERTICAL);
        t.setPadding(dp(14), dp(12), dp(14), dp(12));
        t.setBackground(getDrawable(R.drawable.bg_nav_item));
        t.setClickable(true);
        t.setOnClickListener(v -> { showPage(index); closeDrawer(); });
        return t;
    }

    private void showPage(int index) {
        content.removeAllViews();
        switch (index) {
            case 0: content.addView(new DashboardPage(this)); break;
            case 1: content.addView(new ChatPage(this)); break;
            case 2: content.addView(new ScriptsPage(this)); break;
            case 3: content.addView(new TerminalPage(this)); break;
            case 4: content.addView(new SettingsPage(this)); break;
        }
    }

    private void openDrawer() { translate(0); scrim.setAlpha(0.5f); scrim.setVisibility(View.VISIBLE); }
    private void closeDrawer() { translate(-dp(300)); scrim.setAlpha(0f); scrim.setVisibility(View.GONE); }

    private void translate(int to) {
        drawer.clearAnimation();
        drawer.animate().translationX(to).setDuration(180).start();
    }

    private int dp(int v) { return Math.round(getResources().getDisplayMetrics().density * v); }
    private int colorRes(int c) { return getResources().getColor(c); }
}
