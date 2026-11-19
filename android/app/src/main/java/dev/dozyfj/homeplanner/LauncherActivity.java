package dev.dozyfj.homeplanner;

import android.app.Activity;
import android.app.NativeActivity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Bundle;
import android.provider.Settings;
import android.view.Gravity;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

import androidx.core.content.FileProvider;

import java.io.File;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class LauncherActivity extends Activity {
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
        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        column.setGravity(Gravity.CENTER);
        int padding = Math.round(24 * getResources().getDisplayMetrics().density);
        column.setPadding(padding, padding, padding, padding);
        setContentView(column);

        TextView title = new TextView(this);
        title.setText(R.string.app_name);
        title.setTextSize(27);
        title.setGravity(Gravity.CENTER);
        column.addView(title);

        TextView version = new TextView(this);
        PackageInfo installed = client.installed();
        version.setText(getString(R.string.current_version, installed.versionName));
        version.setGravity(Gravity.CENTER);
        version.setPadding(0, 0, 0, padding);
        column.addView(version);

        Button editor = new Button(this);
        editor.setText(R.string.open_editor);
        editor.setOnClickListener(view -> startActivity(new Intent(this, NativeActivity.class)));
        boolean vulkan = getPackageManager().hasSystemFeature(PackageManager.FEATURE_VULKAN_HARDWARE_VERSION);
        editor.setEnabled(vulkan);
        column.addView(editor);

        updates = new Button(this);
        updates.setOnClickListener(view -> updateAction());
        column.addView(updates);

        status = new TextView(this);
        status.setGravity(Gravity.CENTER);
        status.setPadding(0, padding, 0, 0);
        column.addView(status);

        if (!vulkan) {
            TextView warning = new TextView(this);
            warning.setText(R.string.no_vulkan);
            warning.setGravity(Gravity.CENTER);
            column.addView(warning);
        }
        check(false);
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
