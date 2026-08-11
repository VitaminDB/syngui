package syngui.android;

import android.app.Activity;
import android.content.Context;
import android.text.InputType;
import android.view.*;
import android.view.inputmethod.*;
import android.widget.FrameLayout;
import java.util.concurrent.ConcurrentLinkedQueue;

public class SynGuiInputHandler {
    private static SynGuiInputView inputView;
    private static InputMethodManager imm;
    static final ConcurrentLinkedQueue<String> events = new ConcurrentLinkedQueue<>();
    // Тип клавиатуры: true — цифровая, false — обычная (текст). Package-private,
    // чтобы SynGuiInputView мог прочитать в onCreateInputConnection.
    static volatile boolean numericInput = false;

    public static void setNumericInput(boolean numeric) {
        numericInput = numeric;
        if (inputView != null && imm != null) {
            inputView.post(() -> imm.restartInput(inputView));
        }
    }

    public static void register(Activity activity) {
        activity.runOnUiThread(() -> {
            inputView = new SynGuiInputView(activity);
            inputView.setLayoutParams(new FrameLayout.LayoutParams(1, 1));
            inputView.setAlpha(0f);
            inputView.setFocusable(true);
            inputView.setFocusableInTouchMode(true);

            ViewGroup root = (ViewGroup) activity.getWindow().getDecorView();
            root.addView(inputView);

            imm = (InputMethodManager) activity.getSystemService(Activity.INPUT_METHOD_SERVICE);
        });
    }

    public static void showKeyboard() {
        if (inputView != null) {
            inputView.post(() -> {
                inputView.requestFocus();
                if (imm != null) {
                    imm.showSoftInput(inputView, InputMethodManager.SHOW_IMPLICIT);
                }
            });
        }
    }

    public static void hideKeyboard() {
        if (inputView != null && imm != null) {
            inputView.post(() -> {
                imm.hideSoftInputFromWindow(inputView.getWindowToken(), 0);
                inputView.clearFocus();
            });
        }
    }

    public static void setText(String text) {
        if (inputView != null) {
            inputView.post(() -> {
                inputView.currentText = text;
                inputView.cursorPos = text.length();
                inputView.composingStart = -1;
                inputView.composingEnd = -1;
                if (imm != null) {
                    imm.restartInput(inputView);
                }
            });
        }
    }

    public static String pollEvent() {
        return events.poll();
    }
}

/**
 * Invisible View — anchor for IME. Does not manage text itself.
 */
class SynGuiInputView extends View {
    String currentText = "";
    int cursorPos = 0;
    int composingStart = -1;
    int composingEnd = -1;

    public SynGuiInputView(Context context) {
        super(context);
    }

    @Override
    public boolean onCheckIsTextEditor() {
        return true;
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
        outAttrs.inputType = SynGuiInputHandler.numericInput
                ? InputType.TYPE_CLASS_NUMBER
                : InputType.TYPE_CLASS_TEXT;
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN;
        outAttrs.initialSelStart = cursorPos;
        outAttrs.initialSelEnd = cursorPos;
        return new SynGuiInputConnection(this);
    }
}

/**
 * Custom InputConnection — receives individual IME operations.
 * Properly tracks composing region to avoid double input.
 * Events queued for native polling:
 *   "C:text" — final text to insert at cursor (after removing composing region)
 *   "D:before,after" — delete surrounding text
 *   "K:enter" — Enter key
 */
class SynGuiInputConnection extends BaseInputConnection {
    private final SynGuiInputView view;

    SynGuiInputConnection(SynGuiInputView view) {
        super(view, true);
        this.view = view;
    }

    @Override
    public CharSequence getTextBeforeCursor(int n, int flags) {
        int pos = Math.min(view.cursorPos, view.currentText.length());
        int start = Math.max(0, pos - n);
        return view.currentText.substring(start, pos);
    }

    @Override
    public CharSequence getTextAfterCursor(int n, int flags) {
        int pos = Math.min(view.cursorPos, view.currentText.length());
        int end = Math.min(view.currentText.length(), pos + n);
        return view.currentText.substring(pos, end);
    }

    @Override
    public CharSequence getSelectedText(int flags) {
        return "";
    }

    /** Remove current composing region from internal state (no event emitted). */
    private void removeComposingRegion() {
        if (view.composingStart >= 0 && view.composingEnd > view.composingStart) {
            int start = Math.min(view.composingStart, view.currentText.length());
            int end = Math.min(view.composingEnd, view.currentText.length());
            view.currentText = view.currentText.substring(0, start) + view.currentText.substring(end);
            view.cursorPos = start;
            view.composingStart = -1;
            view.composingEnd = -1;
        }
    }

    @Override
    public boolean setComposingText(CharSequence text, int newCursorPosition) {
        // Remove previous composing text first (no event — it was provisional)
        removeComposingRegion();

        String t = text.toString();
        if (t.isEmpty()) {
            // Composing cleared
            return true;
        }

        // Insert composing text at cursor
        int pos = Math.min(view.cursorPos, view.currentText.length());
        view.currentText = view.currentText.substring(0, pos) + t + view.currentText.substring(pos);
        view.composingStart = pos;
        view.composingEnd = pos + t.length();
        view.cursorPos = view.composingEnd;

        // Send composing text as tentative input — native side shows it
        SynGuiInputHandler.events.add("S:" + t);
        return true;
    }

    @Override
    public boolean finishComposingText() {
        // Composing text becomes final — already inserted, just clear composing markers
        if (view.composingStart >= 0) {
            view.composingStart = -1;
            view.composingEnd = -1;
            SynGuiInputHandler.events.add("F:");
        }
        return true;
    }

    @Override
    public boolean commitText(CharSequence text, int newCursorPosition) {
        // Remove composing region first (it was provisional)
        removeComposingRegion();

        String t = text.toString();
        int pos = Math.min(view.cursorPos, view.currentText.length());
        view.currentText = view.currentText.substring(0, pos) + t + view.currentText.substring(pos);
        view.cursorPos = pos + t.length();

        // Send final committed text
        SynGuiInputHandler.events.add("C:" + t);
        return true;
    }

    @Override
    public boolean deleteSurroundingText(int beforeLength, int afterLength) {
        // Clear composing first
        removeComposingRegion();

        int pos = Math.min(view.cursorPos, view.currentText.length());
        int delStart = Math.max(0, pos - beforeLength);
        int delEnd = Math.min(view.currentText.length(), pos + afterLength);
        view.currentText = view.currentText.substring(0, delStart) + view.currentText.substring(delEnd);
        view.cursorPos = delStart;

        SynGuiInputHandler.events.add("D:" + beforeLength + "," + afterLength);
        return true;
    }

    @Override
    public boolean sendKeyEvent(KeyEvent event) {
        if (event.getAction() == KeyEvent.ACTION_DOWN) {
            if (event.getKeyCode() == KeyEvent.KEYCODE_DEL) {
                if (view.composingStart >= 0) {
                    // Backspace within composing — remove last composing char
                    removeComposingRegion();
                    SynGuiInputHandler.events.add("R:");  // reset composing in native
                } else {
                    int pos = Math.min(view.cursorPos, view.currentText.length());
                    if (pos > 0) {
                        view.currentText = view.currentText.substring(0, pos - 1)
                                + view.currentText.substring(pos);
                        view.cursorPos = pos - 1;
                    }
                    SynGuiInputHandler.events.add("D:1,0");
                }
            } else if (event.getKeyCode() == KeyEvent.KEYCODE_FORWARD_DEL) {
                removeComposingRegion();
                int pos = Math.min(view.cursorPos, view.currentText.length());
                if (pos < view.currentText.length()) {
                    view.currentText = view.currentText.substring(0, pos)
                            + view.currentText.substring(pos + 1);
                }
                SynGuiInputHandler.events.add("D:0,1");
            } else if (event.getKeyCode() == KeyEvent.KEYCODE_ENTER) {
                removeComposingRegion();
                SynGuiInputHandler.events.add("K:enter");
            }
        }
        return true;
    }

    @Override
    public boolean setSelection(int start, int end) {
        view.cursorPos = start;
        return true;
    }
}
