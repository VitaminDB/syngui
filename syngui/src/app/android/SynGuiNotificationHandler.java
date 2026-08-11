package syngui.android;

import android.app.Activity;
import android.app.AlarmManager;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.SystemClock;
import java.util.concurrent.ConcurrentLinkedQueue;

public class SynGuiNotificationHandler {
    private static Activity sActivity;
    private static NotificationManager sManager;
    private static AlarmManager sAlarmManager;
    static final ConcurrentLinkedQueue<String> actionEvents = new ConcurrentLinkedQueue<>();

    private static final String ACTION_NOTIFICATION = "syngui.NOTIFICATION_ACTION";
    private static final String ACTION_ALARM = "syngui.ALARM_FIRE";

    public static void register(Activity activity) {
        sActivity = activity;
        sManager = (NotificationManager) activity.getSystemService(Context.NOTIFICATION_SERVICE);
        sAlarmManager = (AlarmManager) activity.getSystemService(Context.ALARM_SERVICE);

        // Register dynamic BroadcastReceiver for action buttons
        ActionReceiver receiver = new ActionReceiver();
        IntentFilter filter = new IntentFilter(ACTION_NOTIFICATION);
        if (Build.VERSION.SDK_INT >= 33) {
            activity.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED);
        } else {
            activity.registerReceiver(receiver, filter);
        }

        // Register alarm receiver
        AlarmReceiver alarmReceiver = new AlarmReceiver();
        IntentFilter alarmFilter = new IntentFilter(ACTION_ALARM);
        if (Build.VERSION.SDK_INT >= 33) {
            activity.registerReceiver(alarmReceiver, alarmFilter, Context.RECEIVER_NOT_EXPORTED);
        } else {
            activity.registerReceiver(alarmReceiver, alarmFilter);
        }
    }

    // ── Foreground-таймер (живой прогресс-бар) ───────────────────────────

    /**
     * Запустить/обновить foreground-сервис живого прогресс-бара обратного отсчёта.
     * Класс сервиса резолвится по имени через контекст активности, поэтому
     * работает из in-memory DEX (сервис лежит в базовом DEX APK).
     */
    public static void startForegroundTimer(String channelId, String title,
            long startMs, long deadlineMs, String readyText, String waitFmt) {
        if (sActivity == null) return;
        try {
            Intent i = new Intent();
            i.setClassName(sActivity, "syngui.android.SynGuiForegroundService");
            i.putExtra("action", "start");
            i.putExtra("channel", channelId);
            i.putExtra("title", title);
            i.putExtra("start", startMs);
            i.putExtra("deadline", deadlineMs);
            i.putExtra("ready_text", readyText);
            i.putExtra("wait_fmt", waitFmt);
            if (Build.VERSION.SDK_INT >= 26) {
                sActivity.startForegroundService(i);
            } else {
                sActivity.startService(i);
            }
        } catch (Throwable t) {
            // FGS недоступен — вызывающая сторона использует fallback.
        }
    }

    public static void stopForegroundTimer() {
        if (sActivity == null) return;
        try {
            Intent i = new Intent();
            i.setClassName(sActivity, "syngui.android.SynGuiForegroundService");
            i.putExtra("action", "stop");
            sActivity.startService(i);
        } catch (Throwable t) {
        }
    }

    // ── Channels ─────────────────────────────────────────────────────────

    public static void createChannel(String id, String name, String description, int importance) {
        if (sManager == null) return;
        NotificationChannel channel = new NotificationChannel(id, name, importance);
        channel.setDescription(description);
        sManager.createNotificationChannel(channel);
    }

    public static void deleteChannel(String id) {
        if (sManager == null) return;
        sManager.deleteNotificationChannel(id);
    }

    // ── Basic notification ───────────────────────────────────────────────

    public static void notify(int id, String channelId, String title, String text,
                              String bigText, int priority, boolean autoCancel,
                              boolean ongoing, String[] actionLabels) {
        if (sActivity == null || sManager == null) return;

        int icon = sActivity.getApplicationInfo().icon;
        Notification.Builder builder = new Notification.Builder(sActivity, channelId)
                .setSmallIcon(icon)
                .setContentTitle(title)
                .setContentText(text)
                .setAutoCancel(autoCancel)
                .setOngoing(ongoing)
                .setContentIntent(getLaunchIntent(sActivity));

        if (bigText != null && !bigText.isEmpty()) {
            builder.setStyle(new Notification.BigTextStyle().bigText(bigText));
        }

        if (actionLabels != null) {
            for (int i = 0; i < actionLabels.length && i < 3; i++) {
                Intent intent = new Intent(ACTION_NOTIFICATION);
                intent.setPackage(sActivity.getPackageName());
                intent.putExtra("notif_id", id);
                intent.putExtra("action_idx", i);
                PendingIntent pi = PendingIntent.getBroadcast(
                        sActivity,
                        id * 10 + i,
                        intent,
                        PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE
                );
                builder.addAction(0, actionLabels[i], pi);
            }
        }

        sManager.notify(id, builder.build());
    }

    // ── Progress notification ────────────────────────────────────────────

    public static void notifyProgress(int id, String channelId, String title, String text,
                                      int max, int progress, boolean indeterminate) {
        if (sActivity == null || sManager == null) return;

        int icon = sActivity.getApplicationInfo().icon;
        Notification.Builder builder = new Notification.Builder(sActivity, channelId)
                .setSmallIcon(icon)
                .setContentTitle(title)
                .setContentText(text)
                .setOngoing(true)
                .setProgress(max, progress, indeterminate);

        sManager.notify(id, builder.build());
    }

    // ── Chronometer notification ─────────────────────────────────────────

    public static void notifyChronometer(int id, String channelId, String title,
                                         long whenMs, boolean countDown) {
        if (sActivity == null || sManager == null) return;

        int icon = sActivity.getApplicationInfo().icon;
        Notification.Builder builder = new Notification.Builder(sActivity, channelId)
                .setSmallIcon(icon)
                .setContentTitle(title)
                .setUsesChronometer(true)
                .setChronometerCountDown(countDown)
                .setWhen(whenMs)
                .setOngoing(true);

        sManager.notify(id, builder.build());
    }

    // ── Cancel ───────────────────────────────────────────────────────────

    public static void cancel(int id) {
        if (sManager != null) sManager.cancel(id);
    }

    public static void cancelAll() {
        if (sManager != null) sManager.cancelAll();
    }

    // ── Permission ───────────────────────────────────────────────────────

    public static boolean hasPermission() {
        if (sActivity == null) return false;
        if (Build.VERSION.SDK_INT >= 33) {
            return sActivity.checkSelfPermission("android.permission.POST_NOTIFICATIONS")
                    == PackageManager.PERMISSION_GRANTED;
        }
        return true;
    }

    public static void requestPermission() {
        if (sActivity == null) return;
        if (Build.VERSION.SDK_INT >= 33) {
            sActivity.requestPermissions(
                    new String[]{"android.permission.POST_NOTIFICATIONS"}, 9001);
        }
    }

    // ── Scheduled Alarms ─────────────────────────────────────────────────

    /**
     * Schedule a notification to fire after delaySecs seconds, even if app is killed.
     * Uses AlarmManager.setExactAndAllowWhileIdle for reliable delivery.
     */
    public static void scheduleAlarm(int alarmId, int delaySecs,
                                     String channelId, String title, String text) {
        if (sActivity == null || sAlarmManager == null) return;

        Intent intent = new Intent(ACTION_ALARM);
        intent.setPackage(sActivity.getPackageName());
        intent.putExtra("alarm_id", alarmId);
        intent.putExtra("channel_id", channelId);
        intent.putExtra("title", title);
        intent.putExtra("text", text);

        PendingIntent pi = PendingIntent.getBroadcast(
                sActivity,
                alarmId + 5000, // offset to avoid collision with notification IDs
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE
        );

        long triggerAt = SystemClock.elapsedRealtime() + (long) delaySecs * 1000L;
        sAlarmManager.setExactAndAllowWhileIdle(
                AlarmManager.ELAPSED_REALTIME_WAKEUP,
                triggerAt,
                pi
        );
    }

    /**
     * Cancel a previously scheduled alarm.
     */
    public static void cancelAlarm(int alarmId) {
        if (sActivity == null || sAlarmManager == null) return;

        Intent intent = new Intent(ACTION_ALARM);
        intent.setPackage(sActivity.getPackageName());

        PendingIntent pi = PendingIntent.getBroadcast(
                sActivity,
                alarmId + 5000,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE
        );

        sAlarmManager.cancel(pi);
    }

    // ── Launch intent helper ────────────────────────────────────────────

    private static PendingIntent getLaunchIntent(Context context) {
        Intent intent = context.getPackageManager()
                .getLaunchIntentForPackage(context.getPackageName());
        if (intent == null) return null;
        intent.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        return PendingIntent.getActivity(context, 0, intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    // ── Action event polling ─────────────────────────────────────────────

    public static String pollAction() {
        return actionEvents.poll();
    }

    // ── BroadcastReceiver for action buttons ─────────────────────────────

    static class ActionReceiver extends BroadcastReceiver {
        @Override
        public void onReceive(Context context, Intent intent) {
            int notifId = intent.getIntExtra("notif_id", -1);
            int actionIdx = intent.getIntExtra("action_idx", -1);
            if (notifId >= 0 && actionIdx >= 0) {
                actionEvents.add("A:" + notifId + ":" + actionIdx);
            }
        }
    }

    // ── BroadcastReceiver for scheduled alarms ───────────────────────────

    static class AlarmReceiver extends BroadcastReceiver {
        @Override
        public void onReceive(Context context, Intent intent) {
            int alarmId = intent.getIntExtra("alarm_id", -1);
            String channelId = intent.getStringExtra("channel_id");
            String title = intent.getStringExtra("title");
            String text = intent.getStringExtra("text");

            if (alarmId < 0 || channelId == null || title == null) return;

            // Ensure channel exists
            NotificationManager nm = (NotificationManager)
                    context.getSystemService(Context.NOTIFICATION_SERVICE);
            if (nm == null) return;

            int icon = context.getApplicationInfo().icon;
            Notification.Builder nb = new Notification.Builder(context, channelId)
                    .setSmallIcon(icon)
                    .setContentTitle(title)
                    .setContentText(text != null ? text : "")
                    .setAutoCancel(true);

            // Add launch intent so tapping notification opens the app
            Intent launchIntent = context.getPackageManager()
                    .getLaunchIntentForPackage(context.getPackageName());
            if (launchIntent != null) {
                launchIntent.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_CLEAR_TOP);
                PendingIntent pi = PendingIntent.getActivity(context, 0, launchIntent,
                        PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
                nb.setContentIntent(pi);
            }

            Notification notification = nb.build();

            nm.notify(alarmId, notification);

            // Also enqueue event for Rust polling
            actionEvents.add("ALARM:" + alarmId);
        }
    }
}
