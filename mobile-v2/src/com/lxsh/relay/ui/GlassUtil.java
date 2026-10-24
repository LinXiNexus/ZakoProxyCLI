package com.lxsh.relay.ui;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.drawable.BitmapDrawable;
import android.os.Build;
import android.renderscript.Allocation;
import android.renderscript.Element;
import android.renderscript.RenderScript;
import android.renderscript.ScriptIntrinsicBlur;
import android.view.View;

/** Real frosted-glass: capture the layer behind, Gaussian-blur it, dim with a white veil. */
public final class GlassUtil {
    private GlassUtil() {}

    /** Capture the whole view (excluding an optional hidden drawer) into a bitmap. */
    public static Bitmap capture(View view) {
        Bitmap bmp = Bitmap.createBitmap(view.getWidth(), view.getHeight(),
                Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(bmp);
        Paint p = new Paint();
        p.setFilterBitmap(true);
        view.draw(canvas);
        view.invalidate();
        return bmp;
    }

    /** Crop a rect from the source bitmap. */
    public static Bitmap crop(Bitmap src, int left, int top, int w, int h) {
        int cw = Math.min(w, Math.max(0, src.getWidth() - left));
        int ch = Math.min(h, Math.max(0, src.getHeight() - top));
        if (cw <= 0 || ch <= 0) return Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888);
        return Bitmap.createBitmap(src, left, top, cw, ch);
    }

    /** Gaussian blur via RenderScript (works on API 26-30; MIUI supported). */
    public static Bitmap blur(Context context, Bitmap src, float radius) {
        if (radius <= 0f) return src;
        try {
            Bitmap out = src.copy(Bitmap.Config.ARGB_8888, true);
            RenderScript rs = RenderScript.create(context);
            Allocation input = Allocation.createFromBitmap(rs, out,
                    Allocation.MipmapControl.MIPMAP_NONE, Allocation.USAGE_SCRIPT);
            Allocation output = Allocation.createTyped(rs, input.getType());
            ScriptIntrinsicBlur script = ScriptIntrinsicBlur.create(rs, Element.U8_4(rs));
            script.setRadius(Math.min(radius, 25f));
            script.setInput(input);
            script.forEach(output);
            output.copyTo(out);
            script.destroy();
            input.destroy();
            output.destroy();
            rs.destroy();
            return out;
        } catch (Throwable t) {
            return src; // fallback: unblurred
        }
    }

    /** Apply a frosted bitmap (blurred + white veil) as a view background. */
    public static void applyGlass(View target, Context context, Bitmap blurred, float veilAlpha) {
        // draw veil onto a COPY so we never try to read/write the same bitmap
        Bitmap veiled = blurred.copy(Bitmap.Config.ARGB_8888, true);
        Paint veil = new Paint();
        veil.setColor(Color.argb((int) (255f * veilAlpha), 255, 255, 255));
        Canvas c = new Canvas(veiled);
        c.drawRect(0, 0, veiled.getWidth(), veiled.getHeight(), veil);
        BitmapDrawable d = new BitmapDrawable(context.getResources(), veiled);
        target.setBackground(d);
        target.invalidate();
    }
}
