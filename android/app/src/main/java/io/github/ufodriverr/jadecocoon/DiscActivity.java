package io.github.ufodriverr.jadecocoon;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Color;
import android.graphics.Typeface;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.OpenableColumns;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.RandomAccessFile;
import java.nio.charset.StandardCharsets;

/**
 * The first screen of the app: the player's own disc image of Jade Cocoon (Europe,
 * SLES-02201, a raw .bin), chosen with the system's file picker as a PC chooses it with
 * its file window, and copied into the game folder as {@code disc.bin}. With an image
 * there it starts the game at once ({@link MainActivity}); the game sends the player back
 * here, with the reason in {@code disc_problem.txt}, when it cannot read the image.
 */
public class DiscActivity extends Activity {
    /** The copy of the image in the game folder (android.rs, DISC). */
    static final String DISC = "disc.bin";
    /** Why the game did not take the image (android.rs, PROBLEM). */
    static final String PROBLEM = "disc_problem.txt";
    /** The machine-local file of the game that names the image (jc-disc). */
    static final String LOCAL_CONFIG = "jc.local.toml";
    /** Passed by the game when it sends the player back here. */
    static final String ASK = "ask";

    /** Where the source of the released builds is published. */
    static final String SOURCE = "https://github.com/ufodriverr/jade-cocoon-bevy-public-v1";

    private static final int PICK = 1;
    /** A raw CD sector, and the sector of the ISO9660 volume descriptor. */
    private static final int SECTOR = 2352;
    private static final int VOLUME_SECTOR = 16;

    private TextView said;
    private TextView problem;
    private Button choose;
    private ProgressBar progress;
    private final Handler main = new Handler(Looper.getMainLooper());
    private volatile boolean copying;

    /** The game folder: the app's folder on the shared storage, where a PC reaches it by
     *  USB (Android/data/io.github.ufodriverr.jadecocoon/files), or its private one. The
     *  game finds the same one (android.rs, game_folder). */
    static File gameFolder(Context context) {
        File folder = context.getExternalFilesDir(null);
        if (folder == null) {
            folder = context.getFilesDir();
        }
        folder.mkdirs();
        return folder;
    }

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        File folder = gameFolder(this);
        boolean asked = getIntent().getBooleanExtra(ASK, false);
        if (!asked && new File(folder, DISC).isFile() && !new File(folder, PROBLEM).isFile()) {
            startGame();
            return;
        }
        buildScreen();
        String why = readProblem(folder);
        if (why != null) {
            problem.setText(why);
            problem.setVisibility(View.VISIBLE);
        }
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        if (said == null) {
            buildScreen();
        }
        String why = readProblem(gameFolder(this));
        if (why != null) {
            problem.setText(why);
            problem.setVisibility(View.VISIBLE);
        }
    }

    private void buildScreen() {
        float dp = getResources().getDisplayMetrics().density;
        int pad = (int) (24 * dp);

        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        column.setPadding(pad, pad, pad, pad);
        column.setGravity(Gravity.CENTER_HORIZONTAL);
        column.setBackgroundColor(Color.BLACK);

        TextView title = new TextView(this);
        title.setText("Jade Cocoon");
        title.setTextColor(Color.rgb(0x8b, 0xe0, 0xa8));
        title.setTextSize(28);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        title.setGravity(Gravity.CENTER);
        column.addView(title);

        said = new TextView(this);
        said.setText("The game plays from your own disc image of Jade Cocoon: Story of the "
                + "Tamamayu (Europe, SLES-02201), a raw .bin file (MODE2/2352), the same file "
                + "the PC build asks for.\n\nChoose it in the next window. It is copied into the "
                + "game folder once (about 700 MB); the next starts go straight to the game.\n\n"
                + "You must lawfully possess an original game copy and have the right to use it.");
        said.setTextColor(Color.rgb(0xdd, 0xdd, 0xdd));
        said.setTextSize(15);
        said.setPadding(0, pad / 2, 0, pad / 2);
        column.addView(said);

        problem = new TextView(this);
        problem.setTextColor(Color.rgb(0xff, 0x90, 0x80));
        problem.setTextSize(14);
        problem.setVisibility(View.GONE);
        problem.setPadding(0, 0, 0, pad / 2);
        column.addView(problem);

        progress = new ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal);
        progress.setMax(1000);
        progress.setVisibility(View.GONE);
        column.addView(progress, new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT));

        choose = new Button(this);
        choose.setText("Choose disc image (.bin)");
        choose.setAllCaps(false);
        choose.setTextSize(18);
        choose.setOnClickListener(view -> pick());
        LinearLayout.LayoutParams wide = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT);
        wide.topMargin = pad / 2;
        column.addView(choose, wide);

        TextView where = new TextView(this);
        where.setText("Game folder (saves, settings, logs): " + gameFolder(this).getAbsolutePath()
                + "\nA PC reaches it over USB, to copy saves both ways.");
        where.setTextColor(Color.rgb(0x88, 0x88, 0x88));
        where.setTextSize(12);
        where.setPadding(0, pad, 0, 0);
        column.addView(where);

        TextView licence = new TextView(this);
        licence.setText("Jade Cocoon on Bevy " + versionName() + ", a free, unofficial fan project. "
                + "GPL-3.0-only, without warranty; no game files are included. Source, licence and "
                + "third-party notices: " + SOURCE + " (also inside this app: assets/legal).");
        licence.setTextColor(Color.rgb(0x88, 0x88, 0x88));
        licence.setTextSize(12);
        licence.setPadding(0, pad / 2, 0, 0);
        column.addView(licence);

        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        scroll.setBackgroundColor(Color.BLACK);
        scroll.addView(column);
        setContentView(scroll);
    }

    private void pick() {
        if (copying) {
            return;
        }
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        // A .bin has no MIME type of its own: every file is offered.
        intent.setType("*/*");
        startActivityForResult(intent, PICK);
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK || result != RESULT_OK || data == null || data.getData() == null) {
            return;
        }
        copy(data.getData());
    }

    /** Copies the chosen file into the game folder on a thread of its own, with the
     *  progress on screen, checks it and starts the game. */
    private void copy(Uri uri) {
        copying = true;
        choose.setEnabled(false);
        problem.setVisibility(View.GONE);
        progress.setVisibility(View.VISIBLE);
        progress.setProgress(0);
        final long size = sizeOf(uri);
        final File folder = gameFolder(this);
        final File part = new File(folder, DISC + ".part");
        final File disc = new File(folder, DISC);
        said.setText("Copying " + nameOf(uri) + " ...");
        new Thread(() -> {
            String error = null;
            try (InputStream in = getContentResolver().openInputStream(uri);
                 OutputStream out = new FileOutputStream(part)) {
                if (in == null) {
                    throw new IOException("the file cannot be opened");
                }
                byte[] buffer = new byte[1 << 20];
                long done = 0;
                int read;
                long shown = 0;
                while ((read = in.read(buffer)) > 0) {
                    out.write(buffer, 0, read);
                    done += read;
                    if (size > 0 && done - shown > (8 << 20)) {
                        shown = done;
                        final int at = (int) (done * 1000 / size);
                        final long mb = done >> 20;
                        main.post(() -> {
                            progress.setProgress(at);
                            said.setText("Copying ... " + mb + " MB of " + (size >> 20) + " MB");
                        });
                    }
                }
            } catch (IOException err) {
                error = "Copying failed: " + err.getMessage();
            }
            if (error == null) {
                error = checkImage(part);
            }
            if (error == null) {
                disc.delete();
                if (!part.renameTo(disc)) {
                    error = "Cannot move the copy into place.";
                }
            }
            if (error != null) {
                part.delete();
            } else {
                new File(folder, PROBLEM).delete();
                // The game writes it anew for the new image (android.rs, prepare).
                new File(folder, LOCAL_CONFIG).delete();
            }
            final String failed = error;
            main.post(() -> {
                copying = false;
                progress.setVisibility(View.GONE);
                choose.setEnabled(true);
                if (failed != null) {
                    said.setText("Choose your disc image of Jade Cocoon (Europe, SLES-02201), a raw .bin.");
                    problem.setText(failed);
                    problem.setVisibility(View.VISIBLE);
                } else {
                    startGame();
                }
            });
        }).start();
    }

    /** What is wrong with a file as a raw disc image, or null: whole raw sectors and the
     *  ISO9660 volume descriptor in sector 16. The game checks the rest. */
    static String checkImage(File file) {
        long length = file.length();
        if (length < (long) SECTOR * (VOLUME_SECTOR + 1)) {
            return "The file is too small to be a disc image (" + length + " bytes).";
        }
        if (length % SECTOR != 0) {
            return "This is not a raw disc image of 2352-byte sectors (a .bin of MODE2/2352). "
                    + "An .iso of 2048-byte sectors or a .cue does not work: choose the .bin.";
        }
        try (RandomAccessFile raw = new RandomAccessFile(file, "r")) {
            // The sector's header (16 bytes) and MODE2 subheader (8), then the volume
            // descriptor: type 1, "CD001".
            byte[] head = new byte[6];
            raw.seek((long) SECTOR * VOLUME_SECTOR + 24);
            raw.readFully(head);
            String id = new String(head, 1, 5, StandardCharsets.US_ASCII);
            if (head[0] != 1 || !id.equals("CD001")) {
                return "This file is not a PlayStation disc image (no ISO9660 volume found).";
            }
        } catch (IOException err) {
            return "Cannot read the copy: " + err.getMessage();
        }
        return null;
    }

    /** The game: a game that runs already comes to the front as it is (MainActivity is
     *  singleTask); only one that does not starts. */
    private void startGame() {
        Intent intent = new Intent(this, MainActivity.class);
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        startActivity(intent);
        finish();
    }

    private String versionName() {
        try {
            return getPackageManager().getPackageInfo(getPackageName(), 0).versionName;
        } catch (android.content.pm.PackageManager.NameNotFoundException err) {
            return "";
        }
    }

    private static String readProblem(File folder) {
        File file = new File(folder, PROBLEM);
        if (!file.isFile()) {
            return null;
        }
        try (InputStream in = new java.io.FileInputStream(file)) {
            byte[] bytes = in.readAllBytes();
            return "The game could not use the last image:\n" + new String(bytes, StandardCharsets.UTF_8);
        } catch (IOException err) {
            return null;
        }
    }

    private long sizeOf(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, new String[] {OpenableColumns.SIZE}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0)) {
                return cursor.getLong(0);
            }
        } catch (RuntimeException ignored) {
        }
        return -1;
    }

    private String nameOf(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0)) {
                return cursor.getString(0);
            }
        } catch (RuntimeException ignored) {
        }
        return "the disc image";
    }
}
