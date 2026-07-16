package syngui.android;

import android.os.Build;

public class MguiVersionHelper {
    public static int getApiLevel() {
        return Build.VERSION.SDK_INT;
    }
}
