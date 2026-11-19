package dev.dozyfj.homeplanner;

import android.app.NativeActivity;
import android.content.Intent;

public final class PlannerActivity extends NativeActivity {
    public void showUpdates() {
        runOnUiThread(() -> startActivity(new Intent(this, LauncherActivity.class)
                .putExtra(LauncherActivity.ONLY_UPDATES, true)));
    }
}
