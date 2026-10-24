package com.lxsh.relay.ui;

import android.content.Context;
import android.graphics.Typeface;
import android.view.Gravity;
import android.view.View;
import android.widget.LinearLayout;
import android.widget.TextView;

final class UI {
    static LinearLayout page(Context c) {
        LinearLayout p = new LinearLayout(c);
        p.setOrientation(LinearLayout.VERTICAL);
        p.setPadding(dp(c, 20), dp(c, 20), dp(c, 20), dp(c, 20));
        return p;
    }
    static TextView title(Context c, String text) {
        TextView t = new TextView(c);
        t.setText(text);
        t.setTextSize(21);
        t.setTypeface(Typeface.create("sans-serif", Typeface.BOLD));
        t.setTextColor(0xFF187B60);
        return t;
    }
    static TextView subtitle(Context c, String text) {
        TextView t = new TextView(c);
        t.setText(text);
        t.setTextSize(11);
        t.setTextColor(0xFF87918C);
        t.setPadding(0, dp(c, 4), 0, dp(c, 16));
        return t;
    }
    static View panel(Context c, View title, View body) {
        LinearLayout box = new LinearLayout(c);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setBackgroundColor(0xF2FFFFFF);
        box.setElevation(2f * c.getResources().getDisplayMetrics().density);
        box.setPadding(dp(c, 16), dp(c, 14), dp(c, 16), dp(c, 14));
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-1, -2);
        lp.bottomMargin = dp(c, 14);
        if (title != null) box.addView(title);
        if (body != null) box.addView(body);
        box.setLayoutParams(lp);
        return box;
    }
    static TextView empty(Context c, String text) {
        TextView t = new TextView(c);
        t.setText(text);
        t.setTextSize(13);
        t.setTextColor(0xFFA0B0A6);
        t.setGravity(Gravity.CENTER);
        t.setPadding(0, dp(c, 24), 0, dp(c, 24));
        return t;
    }
    static int dp(Context c, int v) { return Math.round(c.getResources().getDisplayMetrics().density * v); }
    static TextView label(Context c, String text) {
        TextView t = new TextView(c);
        t.setText(text);
        t.setTextSize(13);
        t.setTypeface(Typeface.create("sans-serif", Typeface.BOLD));
        t.setTextColor(0xFF27362F);
        return t;
    }
}

final class DashboardPage extends LinearLayout {
    private final TextView status;
    private final TextView stats;

    DashboardPage(Context c) {
        super(c);
        LinearLayout page = UI.page(c);
        page.addView(UI.title(c, "仪表盘"));
        page.addView(UI.subtitle(c, "本地服务状态与用量概览"));

        status = UI.empty(c, "正在检查本地服务…");
        page.addView(UI.panel(c, UI.label(c, "本地服务"), status));

        stats = UI.empty(c, "登录后显示用量");
        page.addView(UI.panel(c, UI.label(c, "用量"), stats));
        addView(page);
        refresh();
    }

    private void refresh() {
        new Thread(() -> {
            try {
                java.net.HttpURLConnection c = (java.net.HttpURLConnection)
                        new java.net.URL("http://127.0.0.1:1991/health").openConnection();
                c.setConnectTimeout(1500);
                c.setReadTimeout(1500);
                boolean ok = c.getResponseCode() == 200;
                c.disconnect();
                android.os.Handler h = new android.os.Handler(android.os.Looper.getMainLooper());
                h.post(() -> status.setText(ok ? "✓ 本地服务运行中" : "✗ 本地服务未运行"));
            } catch (Exception e) {
                android.os.Handler h = new android.os.Handler(android.os.Looper.getMainLooper());
                h.post(() -> status.setText("✗ 无法连接本地服务"));
            }
        }).start();
    }
}

final class ChatPage extends LinearLayout {
    ChatPage(Context c) {
        super(c);
        LinearLayout page = UI.page(c);
        page.addView(UI.title(c, "对话"));
        page.addView(UI.subtitle(c, "流式对话 · M2 里程碑"));
        page.addView(UI.panel(c, UI.label(c, "会话"), UI.empty(c, "M2 里程碑将实现流式输出与工具调用")));
        addView(page);
    }
}

final class ScriptsPage extends LinearLayout {
    ScriptsPage(Context c) {
        super(c);
        LinearLayout page = UI.page(c);
        page.addView(UI.title(c, "临时按键区"));
        page.addView(UI.subtitle(c, "AI 生成的脚本与你的收藏 · M3 里程碑"));
        page.addView(UI.panel(c, UI.label(c, "暂未启用"), UI.empty(c, "M3 里程碑将在此展示脚本卡片网格")));
        addView(page);
    }
}

final class TerminalPage extends LinearLayout {
    TerminalPage(Context c) {
        super(c);
        LinearLayout page = UI.page(c);
        page.addView(UI.title(c, "终端"));
        page.addView(UI.subtitle(c, "内置精简终端 · M4 里程碑"));
        page.addView(UI.panel(c, UI.label(c, "tmoe 引导"), UI.empty(c, "M4 里程碑将在此安装精简 Termux bootstrap")));
        addView(page);
    }
}

final class SettingsPage extends LinearLayout {
    SettingsPage(Context c) {
        super(c);
        LinearLayout page = UI.page(c);
        page.addView(UI.title(c, "设置"));
        page.addView(UI.subtitle(c, "主题 / 对话 / 终端 / 系统"));
        addView(page);
    }
}
