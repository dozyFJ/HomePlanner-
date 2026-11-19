package dev.dozyfj.homeplanner;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.res.ColorStateList;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.RippleDrawable;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Bundle;
import android.provider.Settings;
import android.view.Gravity;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import androidx.core.content.FileProvider;

import java.io.File;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class LauncherActivity extends Activity {
    static final String ONLY_UPDATES = "dev.dozyfj.homeplanner.ONLY_UPDATES";
    private static final int BLUE = 0xFF006C9F;
    private static final int INK = 0xFF152C41;
    private static final int MUTED = 0xFF45647D;
    private static final int BLUE_SURFACE = 0xFFE5F2FB;
    private static final int BORDER = 0xFFD1E4F1;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private UpdateClient client;
    private UpdateClient.Update available;
    private File downloaded;
    private TextView status;
    private Button updates;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        client = new UpdateClient(this);
        boolean onlyUpdates = getIntent().getBooleanExtra(ONLY_UPDATES, false);
        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        LinearLayout page = new LinearLayout(this);
        page.setOrientation(LinearLayout.VERTICAL);
        page.setGravity(Gravity.CENTER);
        page.setPadding(dp(24), dp(24), dp(24), dp(24));
        scroll.addView(page);
        setContentView(scroll);

        LinearLayout heading = new LinearLayout(this);
        heading.setOrientation(LinearLayout.VERTICAL);
        heading.setPadding(dp(20), dp(22), dp(20), dp(22));
        GradientDrawable header = new GradientDrawable(GradientDrawable.Orientation.TL_BR,
                new int[]{0xFF005986, 0xFF087FB7});
        header.setCornerRadius(dp(24));
        heading.setBackground(header);
        page.addView(heading, new LinearLayout.LayoutParams(-1, -2));

        TextView title = new TextView(this);
        title.setText(onlyUpdates ? R.string.update_title : R.string.app_name);
        title.setTextColor(Color.WHITE);
        title.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        title.setTextSize(25);
        heading.addView(title);

        TextView version = new TextView(this);
        PackageInfo installed = client.installed();
        version.setText(getString(R.string.current_version, installed.versionName));
        version.setTextColor(0xFFF0F9FF);
        version.setPadding(0, dp(6), 0, 0);
        heading.addView(version);

        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        column.setPadding(dp(14), dp(14), dp(14), dp(10));
        column.setBackground(surface(Color.WHITE, BORDER, 22));
        LinearLayout.LayoutParams actions = new LinearLayout.LayoutParams(-1, -2);
        actions.topMargin = dp(12);
        page.addView(column, actions);

        boolean vulkan = getPackageManager().hasSystemFeature(PackageManager.FEATURE_VULKAN_HARDWARE_VERSION);
        Button editor = actionButton(true);
        if (onlyUpdates) {
            editor.setText(R.string.return_to_editor);
            editor.setOnClickListener(view -> finish());
        } else {
            editor.setText(R.string.open_editor);
            editor.setOnClickListener(view -> startActivity(new Intent(this, PlannerActivity.class)));
            editor.setEnabled(vulkan);
            if (!vulkan) editor.setAlpha(0.45f);
        }
        column.addView(editor, new LinearLayout.LayoutParams(-1, -2));

        updates = actionButton(false);
        updates.setOnClickListener(view -> updateAction());
        LinearLayout.LayoutParams updateLayout = new LinearLayout.LayoutParams(-1, -2);
        updateLayout.topMargin = dp(8);
        column.addView(updates, updateLayout);

        status = new TextView(this);
        status.setGravity(Gravity.CENTER);
        status.setTextColor(MUTED);
        status.setPadding(0, dp(10), 0, dp(2));
        column.addView(status);

        if (!vulkan && !onlyUpdates) {
            TextView warning = new TextView(this);
            warning.setText(R.string.no_vulkan);
            warning.setGravity(Gravity.CENTER);
            warning.setTextColor(MUTED);
            column.addView(warning);
        }
        check(onlyUpdates);
    }

    private Button actionButton(boolean primary) {
        Button button = new Button(this);
        button.setAllCaps(false);
        button.setTextSize(16);
        button.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        button.setTextColor(primary ? Color.WHITE : BLUE);
        button.setBackground(new RippleDrawable(
                ColorStateList.valueOf(primary ? 0x33FFFFFF : 0x26006C9F),
                surface(primary ? BLUE : BLUE_SURFACE, primary ? BLUE : BORDER, 14), null));
        button.setMinHeight(dp(48));
        button.setPadding(dp(12), dp(8), dp(12), dp(8));
        return button;
    }

    private GradientDrawable surface(int fill, int border, int corner) {
        GradientDrawable drawable = new GradientDrawable();
        drawable.setColor(fill);
        drawable.setCornerRadius(dp(corner));
        drawable.setStroke(dp(1), border);
        return drawable;
    }

    private int dp(int pixels) {
        return Math.round(pixels * getResources().getDisplayMetrics().density);
    }

    private void updateAction() {
        if (downloaded != null && downloaded.isFile()) {
            install();
        } else if (available != null) {
            download(available);
        } else {
            check(true);
        }
    }

    private void check(boolean manual) {
        status.setText(R.string.checking_update);
        updateButton(false);
        worker.execute(() -> {
            try {
                UpdateClient.Update result = client.check();
                post(() -> {
                    available = result;
                    if (result != null) status.setText(getString(R.string.available_update, result.name()));
                    else if (manual) status.setText(R.string.no_update);
                    else status.setText("");
                    updateButton(true);
                });
            } catch (Exception error) {
                post(() -> {
                    status.setText(manual ? getString(R.string.update_error) : "");
                    updateButton(true);
                });
            }
        });
    }

    private void download(UpdateClient.Update release) {
        status.setText(R.string.downloading_update);
        downloaded = null;
        updateButton(false);
        worker.execute(() -> {
            try {
                File apk = client.download(release);
                post(() -> {
                    downloaded = apk;
                    status.setText(R.string.update_ready);
                    updateButton(true);
                    install();
                });
            } catch (Exception error) {
                post(() -> {
                    status.setText(R.string.update_error);
                    updateButton(true);
                });
            }
        });
    }

    private void updateButton(boolean enabled) {
        if (downloaded != null) {
            updates.setText(getString(R.string.install_update, available.name()));
        } else if (available != null) {
            updates.setText(getString(R.string.download_update, available.name()));
        } else {
            updates.setText(R.string.check_update);
        }
        updates.setEnabled(enabled);
        updates.setAlpha(enabled ? 1f : 0.60f);
    }

    private void install() {
        try {
            if (!getPackageManager().canRequestPackageInstalls()) {
                status.setText(R.string.install_permission);
                startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                        Uri.parse("package:" + getPackageName())));
                return;
            }
            Uri uri = FileProvider.getUriForFile(this, getPackageName() + ".updates", downloaded);
            startActivity(new Intent(Intent.ACTION_VIEW)
                    .setDataAndType(uri, "application/vnd.android.package-archive")
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION));
        } catch (ActivityNotFoundException | SecurityException error) {
            status.setText(R.string.update_error);
        }
    }

    private void post(Runnable action) {
        runOnUiThread(() -> {
            if (!isDestroyed() && !isFinishing()) action.run();
        });
    }

    @Override
    protected void onDestroy() {
        worker.shutdownNow();
        super.onDestroy();
    }
}
