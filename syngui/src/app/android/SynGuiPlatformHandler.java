package syngui.android;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.ext.SdkExtensions;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.util.Log;
import android.webkit.MimeTypeMap;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Платформенные службы для syngui::platform: выбор фото/видео системным
 * пикером, «Поделиться» (ACTION_SEND + chooser) и открытие файла (ACTION_VIEW).
 *
 * Класс живёт в in-memory DEX фреймворка, поэтому не может быть ни Activity,
 * ни Fragment (система не найдёт его при пересоздании). Результат активити
 * берётся через ActivityResultRegistry самой GameActivity (это
 * AppCompatActivity → ComponentActivity); androidx-классы видны через
 * родительский загрузчик, но вызываются рефлексией — dex собирается только
 * против android.jar. Результат уходит в Rust нативным методом
 * {@link #nativeOnPicked}, его регистрирует Rust (RegisterNatives).
 */
public class SynGuiPlatformHandler {
    private static final String TAG = "syngui";
    private static Activity sActivity;
    private static final AtomicInteger sSeq = new AtomicInteger();

    /** path == null && error == null — пользователь отменил выбор. */
    static native void nativeOnPicked(long id, String path, String name, String mime, String error);

    public static void register(Activity activity) {
        sActivity = activity;
    }

    private interface ResultHandler {
        void onResult(int resultCode, Intent data);
    }

    // ── Выбор медиа ──────────────────────────────────────────────────────

    /** kind: 0 — фото, 1 — видео, 2 — фото или видео. */
    public static void pickMedia(final long id, final int kind) {
        final Activity a = sActivity;
        if (a == null) {
            nativeOnPicked(id, null, null, null, "Activity ещё не готова");
            return;
        }
        a.runOnUiThread(() -> {
            try {
                launchPicker(a, id, kind);
            } catch (Throwable t) {
                Log.w(TAG, "pickMedia", t);
                nativeOnPicked(id, null, null, null, describe(t));
            }
        });
    }

    private static boolean hasPhotoPicker() {
        if (Build.VERSION.SDK_INT >= 33) return true;
        // Android 11–12: системный пикер приходит обновлением через Play (SDK extension R ≥ 2).
        try {
            return SdkExtensions.getExtensionVersion(Build.VERSION_CODES.R) >= 2;
        } catch (Throwable t) {
            return false;
        }
    }

    private static Intent pickerIntent(int kind) {
        Intent i = new Intent(MediaStore.ACTION_PICK_IMAGES);
        if (kind == 0) i.setType("image/*");
        else if (kind == 1) i.setType("video/*");
        return i;
    }

    private static Intent getContentIntent(int kind) {
        Intent i = new Intent(Intent.ACTION_GET_CONTENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        if (kind == 0) {
            i.setType("image/*");
        } else if (kind == 1) {
            i.setType("video/*");
        } else {
            i.setType("*/*");
            i.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {"image/*", "video/*"});
        }
        return i;
    }

    private static void launchPicker(final Activity a, final long id, final int kind) throws Exception {
        ResultHandler handler = (code, data) -> onPickResult(a, id, code, data);
        if (hasPhotoPicker()) {
            try {
                startForResult(a, pickerIntent(kind), handler);
                return;
            } catch (ActivityNotFoundException e) {
                Log.i(TAG, "photo picker недоступен, ACTION_GET_CONTENT");
            }
        }
        startForResult(a, getContentIntent(kind), handler);
    }

    private static void onPickResult(final Activity a, final long id, int code, Intent data) {
        Uri uri = null;
        if (code == Activity.RESULT_OK && data != null) {
            uri = data.getData();
            if (uri == null && data.getClipData() != null && data.getClipData().getItemCount() > 0) {
                uri = data.getClipData().getItemAt(0).getUri();
            }
        }
        if (uri == null) {
            nativeOnPicked(id, null, null, null, null);
            return;
        }
        final Uri src = uri;
        // Копирование видео может занять секунды — не на UI-потоке.
        new Thread(() -> {
            try {
                copyToCache(a, id, src);
            } catch (Throwable t) {
                Log.w(TAG, "copy picked", t);
                nativeOnPicked(id, null, null, null, describe(t));
            }
        }, "syngui-pick-copy").start();
    }

    private static void copyToCache(Activity a, long id, Uri uri) throws Exception {
        ContentResolver cr = a.getContentResolver();
        String mime = cr.getType(uri);
        String name = null;
        try (Cursor c = cr.query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) name = c.getString(0);
        } catch (Throwable ignored) {
        }
        if (name == null || name.isEmpty()) {
            String seg = uri.getLastPathSegment();
            name = seg != null ? seg : "file";
            String ext = mime != null ? MimeTypeMap.getSingleton().getExtensionFromMimeType(mime) : null;
            if (ext != null && !name.contains(".")) name = name + "." + ext;
        }
        if (mime == null) mime = mimeFor(name);
        String safe = name.replace('/', '_').replace('\\', '_');
        File dir = new File(a.getCacheDir(), "syngui-picked");
        dir.mkdirs();
        File out = new File(dir, System.currentTimeMillis() + "-" + sSeq.incrementAndGet() + "-" + safe);
        try (InputStream in = cr.openInputStream(uri);
             OutputStream os = new FileOutputStream(out)) {
            if (in == null) throw new java.io.IOException("openInputStream вернул null");
            copy(in, os);
        }
        if (needsTranscode(out, mime)) {
            try {
                File jpg = transcodeToJpeg(out);
                out.delete();
                out = jpg;
                mime = "image/jpeg";
                int dot = name.lastIndexOf('.');
                name = (dot > 0 ? name.substring(0, dot) : name) + ".jpg";
            } catch (Throwable t) {
                Log.w(TAG, "перекодирование в JPEG не удалось, отдаю исходник", t);
            }
        }
        nativeOnPicked(id, out.getAbsolutePath(), name, mime, null);
    }

    /**
     * HEIC/HEIF/AVIF (так по умолчанию снимают Samsung и Pixel) отдаются
     * JPEG-ом: такой файл примут любые приложения при «Поделиться» и сборки
     * syngui на ПК без фичи `heif`. Показ от этого не зависит — ImageStore
     * декодирует HEIF/AVIF сам (здесь — системным ImageDecoder), а EXIF-поворот
     * JPEG учитывает, поэтому JPEG отдаётся как есть, без потери качества.
     */
    private static boolean needsTranscode(File f, String mime) {
        if (mime == null) return false;
        switch (mime) {
            case "image/heic":
            case "image/heif":
            case "image/heic-sequence":
            case "image/heif-sequence":
            case "image/avif":
                return true;
            default:
                return false;
        }
    }

    private static File transcodeToJpeg(File src) throws Exception {
        // ImageDecoder сам применяет EXIF-ориентацию.
        android.graphics.Bitmap bmp = android.graphics.ImageDecoder.decodeBitmap(
                android.graphics.ImageDecoder.createSource(src),
                (decoder, info, source) -> decoder.setAllocator(android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE));
        String base = src.getName();
        int dot = base.lastIndexOf('.');
        File out = new File(src.getParentFile(), (dot > 0 ? base.substring(0, dot) : base) + ".jpg");
        try (OutputStream os = new FileOutputStream(out)) {
            if (!bmp.compress(android.graphics.Bitmap.CompressFormat.JPEG, 92, os)) {
                throw new java.io.IOException("Bitmap.compress вернул false");
            }
        } finally {
            bmp.recycle();
        }
        return out;
    }

    // ── ActivityResultRegistry через рефлексию ───────────────────────────

    private static void startForResult(Activity a, Intent intent, final ResultHandler handler) throws Exception {
        ClassLoader cl = a.getClassLoader();
        Object registry;
        try {
            registry = a.getClass().getMethod("getActivityResultRegistry").invoke(a);
        } catch (NoSuchMethodException e) {
            throw new IllegalStateException("Activity без ActivityResultRegistry (нужна ComponentActivity)");
        }
        Class<?> contractBase = Class.forName("androidx.activity.result.contract.ActivityResultContract", true, cl);
        Class<?> contractCls = Class.forName(
                "androidx.activity.result.contract.ActivityResultContracts$StartActivityForResult", true, cl);
        Class<?> cbIface = Class.forName("androidx.activity.result.ActivityResultCallback", true, cl);
        Object contract = contractCls.getConstructor().newInstance();
        final Object[] launcher = new Object[1];
        InvocationHandler ih = (proxy, m, args) -> {
            switch (m.getName()) {
                case "onActivityResult": {
                    Object r = args[0];
                    int code = (Integer) r.getClass().getMethod("getResultCode").invoke(r);
                    Intent data = (Intent) r.getClass().getMethod("getData").invoke(r);
                    try {
                        if (launcher[0] != null) launcher[0].getClass().getMethod("unregister").invoke(launcher[0]);
                    } catch (Throwable ignored) {
                    }
                    handler.onResult(code, data);
                    return null;
                }
                case "hashCode":
                    return System.identityHashCode(proxy);
                case "equals":
                    return proxy == args[0];
                case "toString":
                    return "SynGuiResultCallback";
                default:
                    return null;
            }
        };
        Object cb = Proxy.newProxyInstance(cl, new Class<?>[] {cbIface}, ih);
        String key = "syngui-result-" + sSeq.incrementAndGet();
        Method reg = registry.getClass().getMethod("register", String.class, contractBase, cbIface);
        launcher[0] = reg.invoke(registry, key, contract, cb);
        try {
            launcher[0].getClass().getMethod("launch", Object.class).invoke(launcher[0], intent);
        } catch (InvocationTargetException e) {
            try {
                launcher[0].getClass().getMethod("unregister").invoke(launcher[0]);
            } catch (Throwable ignored) {
            }
            Throwable cause = e.getCause();
            if (cause instanceof Exception) throw (Exception) cause;
            throw e;
        }
    }

    // ── Поделиться / открыть ─────────────────────────────────────────────

    /** null — успешно, иначе текст ошибки. */
    public static String shareText(String title, String text) {
        Activity a = sActivity;
        if (a == null) return "Activity ещё не готова";
        try {
            Intent send = new Intent(Intent.ACTION_SEND);
            send.setType("text/plain");
            send.putExtra(Intent.EXTRA_TEXT, text);
            if (title != null && !title.isEmpty()) {
                send.putExtra(Intent.EXTRA_SUBJECT, title);
                send.putExtra(Intent.EXTRA_TITLE, title);
            }
            a.startActivity(Intent.createChooser(send, title));
            return null;
        } catch (Throwable t) {
            Log.w(TAG, "shareText", t);
            return describe(t);
        }
    }

    public static String shareFile(String path, String mime, String title) {
        Activity a = sActivity;
        if (a == null) return "Activity ещё не готова";
        try {
            File f = new File(path);
            if (!f.isFile()) return "нет файла: " + path;
            if (mime == null || mime.isEmpty()) mime = mimeFor(f.getName());
            Uri uri = contentUri(a, f, mime);
            Intent send = new Intent(Intent.ACTION_SEND);
            send.setType(mime);
            send.putExtra(Intent.EXTRA_STREAM, uri);
            if (title != null && !title.isEmpty()) {
                send.putExtra(Intent.EXTRA_SUBJECT, title);
                send.putExtra(Intent.EXTRA_TITLE, title);
            }
            send.setClipData(ClipData.newRawUri(f.getName(), uri));
            send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            Intent chooser = Intent.createChooser(send, title);
            chooser.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            a.startActivity(chooser);
            return null;
        } catch (Throwable t) {
            Log.w(TAG, "shareFile", t);
            return describe(t);
        }
    }

    public static String openPath(String path) {
        Activity a = sActivity;
        if (a == null) return "Activity ещё не готова";
        try {
            File f = new File(path);
            if (!f.exists()) return "нет файла: " + path;
            String mime = mimeFor(f.getName());
            Uri uri = contentUri(a, f, mime);
            Intent view = new Intent(Intent.ACTION_VIEW);
            view.setDataAndType(uri, mime);
            view.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            a.startActivity(view);
            return null;
        } catch (ActivityNotFoundException e) {
            return "нет приложения для открытия файла";
        } catch (Throwable t) {
            Log.w(TAG, "openPath", t);
            return describe(t);
        }
    }

    /**
     * content:// для своего файла. Основной путь — FileProvider из манифеста
     * приложения (authority «пакет.syngui.files», см. docs). Если провайдера нет
     * или файл вне его корней — копия в MediaStore Downloads (без разрешений
     * на API 29+), её URI выдаётся любому приложению.
     */
    private static Uri contentUri(Context ctx, File f, String mime) throws Exception {
        try {
            Class<?> fp = Class.forName("androidx.core.content.FileProvider", true, ctx.getClassLoader());
            Method m = fp.getMethod("getUriForFile", Context.class, String.class, File.class);
            return (Uri) m.invoke(null, ctx, ctx.getPackageName() + ".syngui.files", f);
        } catch (Throwable t) {
            Log.i(TAG, "FileProvider недоступен (" + describe(t) + "), копия в Downloads");
        }
        ContentValues v = new ContentValues();
        v.put(MediaStore.MediaColumns.DISPLAY_NAME, f.getName());
        v.put(MediaStore.MediaColumns.MIME_TYPE, mime);
        v.put(MediaStore.MediaColumns.RELATIVE_PATH, "Download");
        ContentResolver cr = ctx.getContentResolver();
        Uri uri = cr.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, v);
        if (uri == null) throw new java.io.IOException("MediaStore.insert вернул null");
        try (InputStream in = new FileInputStream(f); OutputStream os = cr.openOutputStream(uri)) {
            if (os == null) throw new java.io.IOException("openOutputStream вернул null");
            copy(in, os);
        }
        return uri;
    }

    // ── Декодирование картинок ───────────────────────────────────────────

    /**
     * HEIF/HEIC (API 28+) и AVIF (API 31+) системным ImageDecoder → RGBA8
     * построчно, для ImageStore syngui (на Android libheif не тянем).
     * Поворот (irot/imir, EXIF) ImageDecoder применяет сам, цвет приводится
     * к sRGB, 10-битные картинки — к ARGB_8888. maxSide > 0 — большая
     * сторона уменьшается до него ещё при декодировании.
     * info[0..2] — ширина, высота, 1 если альфа умножена. Ошибка — null и
     * текст в error[0].
     */
    public static byte[] decodeRgba(byte[] data, final int maxSide, int[] info, String[] error) {
        android.graphics.Bitmap bmp;
        try {
            bmp = decodeBitmap(data, maxSide);
        } catch (Throwable t) {
            error[0] = describe(t);
            return null;
        }
        try {
            if (bmp.getConfig() != android.graphics.Bitmap.Config.ARGB_8888) {
                android.graphics.Bitmap c = bmp.copy(android.graphics.Bitmap.Config.ARGB_8888, false);
                bmp.recycle();
                if (c == null) throw new IllegalStateException("нельзя привести к ARGB_8888");
                bmp = c;
            }
            int w = bmp.getWidth(), h = bmp.getHeight();
            // ARGB_8888 в памяти — байты R, G, B, A.
            java.nio.ByteBuffer buf = java.nio.ByteBuffer.allocate(w * h * 4);
            bmp.copyPixelsToBuffer(buf);
            info[0] = w;
            info[1] = h;
            info[2] = bmp.isPremultiplied() ? 1 : 0;
            bmp.recycle();
            return buf.array();
        } catch (Throwable t) {
            error[0] = describe(t);
            return null;
        }
    }

    private static android.graphics.Bitmap decodeBitmap(byte[] data, final int maxSide)
            throws java.io.IOException {
        android.graphics.ImageDecoder.Source src =
                android.graphics.ImageDecoder.createSource(java.nio.ByteBuffer.wrap(data));
        return android.graphics.ImageDecoder.decodeBitmap(src, (decoder, imgInfo, source) -> {
            decoder.setAllocator(android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE);
            decoder.setUnpremultipliedRequired(true);
            decoder.setTargetColorSpace(
                    android.graphics.ColorSpace.get(android.graphics.ColorSpace.Named.SRGB));
            int w = imgInfo.getSize().getWidth(), h = imgInfo.getSize().getHeight();
            int big = Math.max(w, h);
            if (maxSide > 0 && big > maxSide) {
                float k = maxSide / (float) big;
                decoder.setTargetSize(Math.max(1, Math.round(w * k)), Math.max(1, Math.round(h * k)));
            }
        });
    }

    // ── Вспомогательное ──────────────────────────────────────────────────

    private static String mimeFor(String name) {
        int dot = name.lastIndexOf('.');
        if (dot >= 0) {
            String ext = name.substring(dot + 1).toLowerCase();
            String m = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext);
            if (m != null) return m;
        }
        return "application/octet-stream";
    }

    private static void copy(InputStream in, OutputStream os) throws java.io.IOException {
        byte[] buf = new byte[64 * 1024];
        int n;
        while ((n = in.read(buf)) > 0) os.write(buf, 0, n);
    }

    private static String describe(Throwable t) {
        if (t instanceof InvocationTargetException && t.getCause() != null) t = t.getCause();
        String msg = t.getMessage();
        return t.getClass().getSimpleName() + (msg != null ? ": " + msg : "");
    }
}
