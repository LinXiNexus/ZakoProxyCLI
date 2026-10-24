package com.lxsh.relay.data;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import java.util.Iterator;
import java.util.Map;

/** Small JSON helpers built on org.json (bundled with Android). */
public final class Json {
    private Json() {}

    public static JSONObject obj(String raw) {
        try { return new JSONObject(raw); } catch (JSONException e) { return new JSONObject(); }
    }

    public static JSONObject merge(JSONObject base, JSONObject patch) {
        JSONObject out = new JSONObject();
        Iterator<String> it = base.keys();
        while (it.hasNext()) {
            String k = it.next();
            try { out.put(k, base.get(k)); } catch (JSONException ignored) {}
        }
        Iterator<String> pit = patch.keys();
        while (pit.hasNext()) {
            String k = pit.next();
            try { out.put(k, patch.get(k)); } catch (JSONException ignored) {}
        }
        return out;
    }

    public static JSONObject mapToObj(Map<String, String> m) {
        JSONObject o = new JSONObject();
        for (Map.Entry<String, String> e : m.entrySet()) {
            try { o.put(e.getKey(), e.getValue()); } catch (JSONException ignored) {}
        }
        return o;
    }

    public static JSONArray strings(Iterable<String> values) {
        JSONArray a = new JSONArray();
        for (String v : values) a.put(v);
        return a;
    }
}
