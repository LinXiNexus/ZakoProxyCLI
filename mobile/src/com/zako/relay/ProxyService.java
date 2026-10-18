package com.zako.relay;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Intent;
import android.content.res.AssetManager;
import android.os.Build;
import android.os.IBinder;
import android.util.Log;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;

public class ProxyService extends Service {
    private static final String TAG = "ZakoProxy";
    private static final String CHANNEL = "zako_proxy";
    private Process process;

    @Override
    public void onCreate() {
        super.onCreate();
        startForegroundCompat();
        new Thread(this::prepareAndStart, "zako-boot").start();
    }

    private void startForegroundCompat() {
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationManager nm = getSystemService(NotificationManager.class);
            NotificationChannel ch = new NotificationChannel(
                    CHANNEL, "ZakoAI 本地服务", NotificationManager.IMPORTANCE_LOW);
            ch.setShowBadge(false);
            nm.createNotificationChannel(ch);
        }
        Notification.Builder b;
        if (Build.VERSION.SDK_INT >= 26) {
            b = new Notification.Builder(this, CHANNEL);
        } else {
            b = new Notification.Builder(this);
        }
        b.setContentTitle("ZakoAI 正在运行")
                .setContentText("本地代理与控制台已启动")
                .setSmallIcon(android.R.drawable.stat_sys_download)
                .setOngoing(true);
        startForeground(1, b.build());
    }

    private void prepareAndStart() {
        try {
            File files = getFilesDir();
            File web = new File(files, "web");
            String version = getPackageManager().getPackageInfo(getPackageName(), 0).versionName;
            File marker = new File(files, ".web-" + version);
            if (!marker.exists() || !new File(web, "index.html").exists()) {
                deleteRecursive(web);
                copyAssetDir("web", web);
                //noinspection ResultOfMethodCallIgnored
                marker.createNewFile();
            }
            File data = new File(files, "data");
            //noinspection ResultOfMethodCallIgnored
            data.mkdirs();

            File bin = new File(getApplicationInfo().nativeLibraryDir, "libzakoai.so");
            if (!bin.exists()) {
                Log.e(TAG, "relay binary missing at " + bin);
                return;
            }
            //noinspection ResultOfMethodCallIgnored
            bin.setExecutable(true, false);

            ProcessBuilder pb = new ProcessBuilder(bin.getAbsolutePath());
            pb.directory(files);
            pb.redirectErrorStream(true);
            pb.environment().put("DATA_DIR", data.getAbsolutePath());
            pb.environment().put("WEB_DIR", web.getAbsolutePath());
            pb.environment().put("BIND_ADDR", "127.0.0.1:1990");
            pb.environment().put("RUST_LOG", "info");
            pb.environment().put("HOME", files.getAbsolutePath());
            File log = new File(files, "proxy.log");
            pb.redirectOutput(ProcessBuilder.Redirect.appendTo(log));
            process = pb.start();
            Log.i(TAG, "relay started: " + bin.getAbsolutePath());
        } catch (Throwable t) {
            Log.e(TAG, "failed to start relay", t);
        }
    }

    private void copyAssetDir(String assetPath, File target) throws IOException {
        AssetManager am = getAssets();
        String[] children = am.list(assetPath);
        if (children != null && children.length > 0) {
            //noinspection ResultOfMethodCallIgnored
            target.mkdirs();
            for (String child : children) {
                copyAssetDir(assetPath + "/" + child, new File(target, child));
            }
            return;
        }
        try {
            copyAssetFile(assetPath, target);
        } catch (IOException e) {
            //noinspection ResultOfMethodCallIgnored
            target.mkdirs();
        }
    }

    private void copyAssetFile(String assetPath, File target) throws IOException {
        File parent = target.getParentFile();
        if (parent != null) {
            //noinspection ResultOfMethodCallIgnored
            parent.mkdirs();
        }
        try (InputStream in = getAssets().open(assetPath);
             OutputStream out = new FileOutputStream(target)) {
            byte[] buf = new byte[8192];
            int n;
            while ((n = in.read(buf)) > 0) {
                out.write(buf, 0, n);
            }
        }
    }

    private static void deleteRecursive(File f) {
        if (f.isDirectory()) {
            File[] kids = f.listFiles();
            if (kids != null) {
                for (File kid : kids) deleteRecursive(kid);
            }
        }
        //noinspection ResultOfMethodCallIgnored
        f.delete();
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        if (process != null) {
            process.destroy();
            process = null;
        }
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
