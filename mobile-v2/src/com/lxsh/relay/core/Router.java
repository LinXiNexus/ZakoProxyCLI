package com.lxsh.relay.core;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** Minimal HTTP router: method + path pattern with {param} segments. */
public final class Router {
    public interface Handler {
        Response handle(Request req) throws Exception;
    }

    public static final class Request {
        public final String method;
        public final String path;
        public final Map<String, String> params = new LinkedHashMap<>();
        public final Map<String, List<String>> headers;
        public final byte[] body;

        Request(String method, String path, Map<String, List<String>> headers, byte[] body) {
            this.method = method;
            this.path = path;
            this.headers = headers;
            this.body = body;
        }

        public String header(String name) {
            List<String> v = headers.get(name.toLowerCase());
            return v == null || v.isEmpty() ? null : v.get(0);
        }

        public String cookie(String name) {
            String raw = header("cookie");
            if (raw == null) return null;
            for (String part : raw.split(";")) {
                String[] kv = part.trim().split("=", 2);
                if (kv.length == 2 && kv[0].equals(name)) return kv[1];
            }
            return null;
        }
    }

    public static final class Response {
        public int status = 200;
        public final Map<String, String> headers = new LinkedHashMap<>();
        public byte[] body = new byte[0];
        public boolean sse = false; // stream via chunked SSE
        public SseStream sseStream;

        public static Response json(int status, String json) {
            Response r = new Response();
            r.status = status;
            r.headers.put("Content-Type", "application/json");
            r.body = json.getBytes(java.nio.charset.StandardCharsets.UTF_8);
            return r;
        }

        public static Response sse(SseStream stream) {
            Response r = new Response();
            r.sse = true;
            r.sseStream = stream;
            return r;
        }
    }

    public interface SseStream {
        /** Blocking call: write one SSE frame; return false to end stream. */
        boolean write(java.io.OutputStream out) throws Exception;
    }

    private static final class Route {
        final String method;
        final String[] segments;
        final Handler handler;

        Route(String method, String pattern, Handler handler) {
            this.method = method;
            this.segments = pattern.startsWith("/") ? pattern.substring(1).split("/") : pattern.split("/");
            this.handler = handler;
        }
    }

    private final List<Route> routes = new ArrayList<>();

    public void get(String pattern, Handler h) { add("GET", pattern, h); }
    public void post(String pattern, Handler h) { add("POST", pattern, h); }
    public void put(String pattern, Handler h) { add("PUT", pattern, h); }
    public void delete(String pattern, Handler h) { add("DELETE", pattern, h); }

    private void add(String method, String pattern, Handler h) {
        routes.add(new Route(method, pattern, h));
    }

    /** @return null if no route matched (caller returns 404). */
    public Handler match(String method, String path, Map<String, String> outParams) {
        String[] segs = path.startsWith("/") ? path.substring(1).split("/") : path.split("/");
        for (Route r : routes) {
            if (!r.method.equals(method)) continue;
            if (r.segments.length != segs.length) continue;
            Map<String, String> params = new LinkedHashMap<>();
            boolean ok = true;
            for (int i = 0; i < segs.length; i++) {
                String pat = r.segments[i];
                if (pat.startsWith("{") && pat.endsWith("}")) {
                    params.put(pat.substring(1, pat.length() - 1), segs[i]);
                } else if (!pat.equals(segs[i])) {
                    ok = false;
                    break;
    }
            }
            if (ok) {
                outParams.putAll(params);
                return r.handler;
            }
        }
        return null;
    }
}
