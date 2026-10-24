package com.zako.relay;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.graphics.Color;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.Gravity;
import android.view.View;
import android.webkit.WebChromeClient;
import android.webkit.ValueCallback;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import android.widget.TextView;

import java.io.File;
import java.io.RandomAccessFile;
import java.net.HttpURLConnection;
import java.net.URL;

public class MainActivity extends Activity {
    private static final String BASE = "http://127.0.0.1:1990";
    private WebView webView;
    private TextView status;
    private ValueCallback<Uri[]> fileCallback;
    private static final int FILE_CHOOSER = 42;
    private FrameLayout root;
    private final Handler handler = new Handler(Looper.getMainLooper());

    @SuppressLint("SetJavaScriptEnabled")
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        root = new FrameLayout(this);
        root.setBackgroundColor(Color.WHITE);

        webView = new WebView(this);
        WebSettings s = webView.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        s.setDatabaseEnabled(true);
        s.setCacheMode(WebSettings.LOAD_NO_CACHE);
        s.setAllowFileAccess(false);
        s.setAllowContentAccess(false);
        s.setSupportZoom(false);
        s.setBuiltInZoomControls(false);
        s.setDisplayZoomControls(false);
        s.setUseWideViewPort(false);
        s.setLoadWithOverviewMode(false);
        try {
            WebSettings.class.getMethod("setForceEnableZoom", boolean.class).invoke(s, false);
        } catch (Exception ignored) {
        }
        s.setTextZoom(100);
        webView.setLayerType(android.view.View.LAYER_TYPE_HARDWARE, null);
        if (Build.VERSION.SDK_INT >= 26) {
            webView.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO);
        }
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                Uri uri = request.getUrl();
                String host = uri.getHost();
                if ("http".equals(uri.getScheme()) || "https".equals(uri.getScheme())) {
                    if ("127.0.0.1".equals(host) || "localhost".equals(host)) {
                        return false;
                    }
                    try {
                        startActivity(new Intent(Intent.ACTION_VIEW, uri));
                    } catch (ActivityNotFoundException ignored) {
                    }
                    return true;
                }
                return true;
            }
        });
        webView.setWebChromeClient(new WebChromeClient() {
            @Override
            public boolean onShowFileChooser(WebView view,
                    ValueCallback<Uri[]> callback,
                    FileChooserParams params) {
                if (fileCallback != null) {
                    fileCallback.onReceiveValue(null);
                }
                fileCallback = callback;
                try {
                    Intent intent = params.createIntent();
                    startActivityForResult(intent, FILE_CHOOSER);
                } catch (ActivityNotFoundException e) {
                    fileCallback = null;
                    return false;
                }
                return true;
            }
        });
        root.addView(webView, new FrameLayout.LayoutParams(-1, -1));

        status = new TextView(this);
        status.setTextColor(Color.parseColor("#5a6660"));
        status.setTextSize(15);
        status.setGravity(Gravity.CENTER);
        status.setBackgroundColor(Color.WHITE);
        status.setText("正在启动本地服务…\n首次启动需要解压前端资源，请稍候");
        root.addView(status, new FrameLayout.LayoutParams(-1, -1));

        setContentView(root);

        Intent intent = new Intent(this, ProxyService.class);
        if (Build.VERSION.SDK_INT >= 26) startForegroundService(intent);
        else startService(intent);

        waitForServer();
    }

    private void waitForServer() {
        new Thread(() -> {
            boolean ok = false;
            for (int i = 0; i < 90; i++) {
                if (probe()) {
                    ok = true;
                    break;
                }
                try {
                    Thread.sleep(1000);
                } catch (InterruptedException ignored) {
                }
            }
            final boolean ready = ok;
            handler.post(() -> {
                if (ready) {
                    status.setVisibility(View.GONE);
                    webView.loadUrl(BASE + "/");
                } else {
                    status.setText("本地服务启动失败\n\n最近日志：\n" + tailLog());
                }
            });
        }, "lxsh-health").start();
    }

    private boolean probe() {
        HttpURLConnection c = null;
        try {
            c = (HttpURLConnection) new URL(BASE + "/health").openConnection();
            c.setConnectTimeout(1500);
            c.setReadTimeout(1500);
            return c.getResponseCode() == 200;
        } catch (Exception e) {
            return false;
        } finally {
            if (c != null) c.disconnect();
        }
    }

    private String tailLog() {
        File f = new File(getFilesDir(), "proxy.log");
        if (!f.exists()) return "（暂无日志）";
        try (RandomAccessFile raf = new RandomAccessFile(f, "r")) {
            long len = raf.length();
            long start = Math.max(0, len - 4000);
            raf.seek(start);
            byte[] buf = new byte[(int) (len - start)];
            raf.readFully(buf);
            return new String(buf, "UTF-8");
        } catch (Exception e) {
            return "读取日志失败：" + e.getMessage();
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        if (requestCode == FILE_CHOOSER && fileCallback != null) {
            Uri[] results = WebChromeClient.FileChooserParams.parseResult(resultCode, data);
            fileCallback.onReceiveValue(results);
            fileCallback = null;
            return;
        }
        super.onActivityResult(requestCode, resultCode, data);
    }

    @Override
    public void onBackPressed() {
        if (webView != null && webView.canGoBack()) {
            webView.goBack();
        } else {
            moveTaskToBack(true);
        }
    }

    @Override
    protected void onDestroy() {
        if (webView != null) {
            root.removeView(webView);
            webView.destroy();
            webView = null;
        }
        super.onDestroy();
    }
}
