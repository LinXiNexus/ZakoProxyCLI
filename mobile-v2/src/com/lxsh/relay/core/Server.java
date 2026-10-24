package com.lxsh.relay.core;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Tiny embedded HTTP/1.1 server with keep-alive and SSE streaming support. */
public final class Server {
    public interface ErrorBody {
        String body(int status);
    }

    private final Router router;
    private final ExecutorService pool = Executors.newCachedThreadPool();
    private volatile ServerSocket serverSocket;
    private volatile boolean running;

    public Server(Router router) {
        this.router = router;
    }

    public synchronized void start(int port) throws IOException {
        if (running) return;
        serverSocket = new ServerSocket();
        serverSocket.setReuseAddress(true);
        serverSocket.bind(new InetSocketAddress("127.0.0.1", port), 64);
        running = true;
        Thread acceptor = new Thread(this::acceptLoop, "lxsh-server");
        acceptor.setDaemon(true);
        acceptor.start();
    }

    public synchronized void stop() {
        running = false;
        try {
            if (serverSocket != null) serverSocket.close();
        } catch (IOException ignored) {}
        pool.shutdownNow();
    }

    public boolean isRunning() { return running; }

    private void acceptLoop() {
        while (running) {
            try {
                Socket sock = serverSocket.accept();
                sock.setTcpNoDelay(true);
                pool.execute(() -> serve(sock));
            } catch (IOException e) {
                if (running) continue; else break;
            }
        }
    }

    private void serve(Socket sock) {
        try (Socket s = sock) {
            s.setSoTimeout(30000);
            BufferedReader reader = new BufferedReader(new InputStreamReader(s.getInputStream(), StandardCharsets.ISO_8859_1));
            while (running) {
                String line = reader.readLine();
                if (line == null || line.isEmpty()) continue;
                String[] parts = line.split(" ");
                if (parts.length < 2) break;
                String method = parts[0];
                String target = parts[1];
                Map<String, List<String>> headers = new HashMap<>();
                int len = 0;
                while ((line = reader.readLine()) != null && !line.isEmpty()) {
                    int idx = line.indexOf(':');
                    if (idx > 0) {
                        String k = line.substring(0, idx).trim().toLowerCase(Locale.ROOT);
                        String v = line.substring(idx + 1).trim();
                        headers.computeIfAbsent(k, x -> new ArrayList<>()).add(v);
                        if (k.equals("content-length")) {
                            try { len = Integer.parseInt(v); } catch (NumberFormatException ignored) {}
                        }
                    }
                }
                byte[] body = len > 0 ? readFully(s.getInputStream(), len) : new byte[0];
                String path = target;
                String query = null;
                int q = target.indexOf('?');
                if (q >= 0) { path = target.substring(0, q); query = target.substring(q + 1); }
                Map<String, String> params = new HashMap<>();
                Router.Handler h = router.match(method, path, params);
                Router.Response resp;
                if (h == null) {
                    resp = Router.Response.json(404, "{\"error\":\"Not Found\"}");
                } else {
                    Router.Request req = new Router.Request(method, path, headers, body);
                    try {
                        resp = h.handle(req);
                    } catch (Exception e) {
                        resp = Router.Response.json(500, "{\"error\":\"" + esc(e.getMessage()) + "\"}");
                    }
                }
                writeResponse(s.getOutputStream(), resp, "keep-alive".equalsIgnoreCase(headers.getOrDefault("connection", List.of("keep-alive")).get(0)) || true);
                if (resp.sse) {
                    // stream until handler ends
                    OutputStream os = s.getOutputStream();
                    while (resp.sseStream.write(os)) { os.flush(); }
                    os.flush();
                    return; // SSE connection finished; close
                }
            }
        } catch (Exception ignored) {
        }
    }

    static byte[] readFully(InputStream in, int len) throws IOException {
        byte[] buf = new byte[len];
        int off = 0;
        while (off < len) {
            int n = in.read(buf, off, len - off);
            if (n < 0) break;
            off += n;
        }
        return buf;
    }

    private void writeResponse(OutputStream out, Router.Response r, boolean keepAlive) throws IOException {
        StringBuilder head = new StringBuilder();
        head.append("HTTP/1.1 ").append(r.status).append(" ").append(reason(r.status)).append("\r\n");
        if (r.sse) {
            head.append("Content-Type: text/event-stream\r\n");
            head.append("Cache-Control: no-cache\r\n");
            head.append("Connection: close\r\n\r\n");
            out.write(head.toString().getBytes(StandardCharsets.ISO_8859_1));
            out.flush();
            return;
        }
        if (!r.headers.containsKey("Content-Type")) r.headers.put("Content-Type", "application/json");
        r.headers.put("Content-Length", String.valueOf(r.body.length));
        r.headers.put("Cache-Control", "no-store");
        r.headers.put("Connection", "close");
        for (Map.Entry<String, String> e : r.headers.entrySet()) {
            head.append(e.getKey()).append(": ").append(e.getValue()).append("\r\n");
        }
        head.append("\r\n");
        out.write(head.toString().getBytes(StandardCharsets.ISO_8859_1));
        out.write(r.body);
        out.flush();
    }

    private static String reason(int s) {
        switch (s) {
            case 200: return "OK";
            case 201: return "Created";
            case 204: return "No Content";
            case 400: return "Bad Request";
            case 401: return "Unauthorized";
            case 403: return "Forbidden";
            case 404: return "Not Found";
            case 409: return "Conflict";
            case 429: return "Too Many Requests";
            case 500: return "Internal Server Error";
            case 502: return "Bad Gateway";
            case 503: return "Service Unavailable";
            default: return "Error";
        }
    }

    private static String esc(String s) {
        if (s == null) return "";
        return s.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", " ");
    }
}
