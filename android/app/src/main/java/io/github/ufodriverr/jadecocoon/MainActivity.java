package io.github.ufodriverr.jadecocoon;

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.Bundle;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.view.View;
import android.view.WindowManager;

import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;

import com.google.androidgamesdk.GameActivity;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/**
 * The game: Bevy runs in {@code libjadecocoon.so} (android.rs). This class keeps the
 * screen whole (no bars, no sleep) and offers the game what only Java reaches: the
 * clipboard, a tick of the vibration and the way back to the disc screen.
 */
public class MainActivity extends GameActivity {
    static {
        System.loadLibrary("jadecocoon");
    }

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        hideBars();
    }

    /** The game lives on the thread this activity started (android.rs): once the activity
     *  is gone the game is too, and the process ends with it, so that the next start is
     *  a fresh one and never a second game in the same process. */
    @Override
    protected void onDestroy() {
        super.onDestroy();
        android.os.Process.killProcess(android.os.Process.myPid());
    }

    @Override
    public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        if (focused) {
            hideBars();
        }
    }

    private void hideBars() {
        View decor = getWindow().getDecorView();
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
        WindowInsetsControllerCompat bars = WindowCompat.getInsetsController(getWindow(), decor);
        bars.hide(WindowInsetsCompat.Type.systemBars());
        bars.setSystemBarsBehavior(WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
        if (Build.VERSION.SDK_INT >= 28) {
            WindowManager.LayoutParams attributes = getWindow().getAttributes();
            attributes.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(attributes);
        }
    }

    /** The system's back gesture is the game's (the triangle, android.rs / touch.rs): it
     *  never closes the game by accident. */
    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
    }

    /** Called by the game (android.rs): the text goes to the clipboard. */
    public void setClipboard(String text) {
        runOnUiThread(() -> {
            ClipboardManager board = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            if (board != null) {
                board.setPrimaryClip(ClipData.newPlainText("Jade Cocoon", text));
            }
        });
    }

    /** Called by the game (android.rs): the text on the clipboard, or null. */
    public String getClipboard() {
        AtomicReference<String> text = new AtomicReference<>();
        CountDownLatch done = new CountDownLatch(1);
        runOnUiThread(() -> {
            try {
                ClipboardManager board = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                if (board != null && board.hasPrimaryClip()) {
                    ClipData clip = board.getPrimaryClip();
                    if (clip != null && clip.getItemCount() > 0) {
                        CharSequence item = clip.getItemAt(0).coerceToText(this);
                        if (item != null) {
                            text.set(item.toString());
                        }
                    }
                }
            } finally {
                done.countDown();
            }
        });
        try {
            done.await(2, TimeUnit.SECONDS);
        } catch (InterruptedException ignored) {
        }
        return text.get();
    }

    /** Called by the game (touch.rs): a short tick under the thumb. */
    @SuppressWarnings("deprecation")
    public void tick() {
        Vibrator vibrator = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
        if (vibrator != null && vibrator.hasVibrator()) {
            vibrator.vibrate(VibrationEffect.createPredefined(VibrationEffect.EFFECT_TICK));
        }
    }

    /** Called by the game (android.rs) when it cannot read the disc image: the disc
     *  screen comes back with the reason, and this process ends with the game. */
    public void askForDisc() {
        Intent intent = new Intent(this, DiscActivity.class);
        intent.putExtra(DiscActivity.ASK, true);
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
        startActivity(intent);
    }
}
