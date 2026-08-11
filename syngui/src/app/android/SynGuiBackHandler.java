package syngui.android;

import android.app.Activity;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;
import java.util.concurrent.atomic.AtomicBoolean;

public class SynGuiBackHandler {
    public static final AtomicBoolean backPressed = new AtomicBoolean(false);

    public static void register(Activity activity) {
        OnBackInvokedCallback callback = () -> backPressed.set(true);
        activity.getWindow().getOnBackInvokedDispatcher()
                .registerOnBackInvokedCallback(
                        OnBackInvokedDispatcher.PRIORITY_OVERLAY, callback);
    }

    public static boolean consumeBack() {
        return backPressed.getAndSet(false);
    }
}
