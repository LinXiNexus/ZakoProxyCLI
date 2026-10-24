package com.lxsh.relay.core;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;
import android.util.Log;
import com.lxsh.relay.data.Api;
import com.lxsh.relay.data.Db;

/** Hosts the embedded LXSH HTTP server (console + proxy API) in the foreground. */
public final class ProxyService extends Service {
    private static final String TAG = "LXSH-Server";
    private static final String CHANNEL = "lxsh_proxy";
    public static final int SERVER_PORT = 1991;

    private Db db;
    private Server server;

    @Override
    public void onCreate() {
        super.onCreate();
        startForegroundInternal();
        db = new Db();
        db.open(this); // opens (or migrates + creates) lxsh.db
        Router router = new Router();
        new Api(db, this).register(router);
        server = new Server(router);
        try {
            server.start(SERVER_PORT);
            Log.i(TAG, "LXSH server listening on 127.0.0.1:" + SERVER_PORT);
        } catch (Exception e) {
            Log.e(TAG, "server start failed", e);
        }
    }

    private void startForegroundInternal() {
        NotificationManager nm = (NotificationManager) getSystemService(NOTIFICATION_SERVICE);
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationChannel ch = new NotificationChannel(CHANNEL, "LXSH 本地服务", NotificationManager.IMPORTANCE_LOW);
            nm.createNotificationChannel(ch);
        }
        Notification.Builder b = Build.VERSION.SDK_INT >= 26
                ? new Notification.Builder(this, CHANNEL) : new Notification.Builder(this);
        b.setContentTitle("LXSH 正在运行")
                .setContentText("本地控制台已启动")
                .setSmallIcon(android.R.drawable.stat_sys_download)
                .setOngoing(true);
        Notification n = b.build();
        startForeground(1, n);
    }

    @Override
    public IBinder onBind(Intent intent) { return null; }

    @Override
    public void onDestroy() {
        if (server != null) server.stop();
        if (db != null) db.close();
        super.onDestroy();
    }
}
