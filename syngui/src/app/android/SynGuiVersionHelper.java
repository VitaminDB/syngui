package syngui.android;

import android.os.Build;

public class SynGuiVersionHelper {
    public static int getApiLevel() {
        return Build.VERSION.SDK_INT;
    }
}
