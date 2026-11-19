package dev.dozyfj.homeplanner;

import android.app.Activity;
import android.app.AlertDialog;
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
import android.widget.EditText;
import android.widget.Switch;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import androidx.core.content.FileProvider;

import java.io.File;
import java.io.IOException;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class LauncherActivity extends Activity {
    static final String ONLY_UPDATES = "dev.dozyfj.homeplanner.ONLY_UPDATES";
    private static final int BLUE = 0xFF006C9F;
    static final String RETURN_TO_EDITOR = "dev.dozyfj.homeplanner.RETURN_TO_EDITOR";
    static final String AUTO_OPEN_PROJECTS = "dev.dozyfj.homeplanner.AUTO_OPEN_PROJECTS";
    static final String PROJECT_CLOSED = "dev.dozyfj.homeplanner.PROJECT_CLOSED";
    static final String AUTO_CREATE_PROJECT = "dev.dozyfj.homeplanner.AUTO_CREATE_PROJECT";
    static final String PROJECT_NAME = "dev.dozyfj.homeplanner.PROJECT_NAME";
    static final String PROJECT_ID = "dev.dozyfj.homeplanner.PROJECT_ID";
    static final String PROJECT_MODEL = "dev.dozyfj.homeplanner.PROJECT_MODEL";
    private static final int MUTED = 0xFF45647D;
    private static final int BLUE_SURFACE = 0xFFE5F2FB;
    private static final int BORDER = 0xFFD1E4F1;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private UpdateClient client;
    private UpdateClient.Update available;
    private File downloaded;
    private TextView status;
    private Button updates;
    private boolean dark;
    private Projects projects;

    @Override
    protected void onCreate(Bundle state) {
        dark = Projects.dark(this);
        if (dark) setTheme(R.style.HomePlannerDarkTheme);
        super.onCreate(state);
        client = new UpdateClient(this);
        projects = new Projects(this);
        boolean onlyUpdates = getIntent().getBooleanExtra(ONLY_UPDATES, false);
        boolean returnToEditor = onlyUpdates || getIntent().getBooleanExtra(RETURN_TO_EDITOR, false);
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
        column.setBackground(surface(dark ? 0xFF1E3143 : Color.WHITE, border(), 22));
        LinearLayout.LayoutParams actions = new LinearLayout.LayoutParams(-1, -2);
        actions.topMargin = dp(12);
        page.addView(column, actions);

        boolean vulkan = getPackageManager().hasSystemFeature(PackageManager.FEATURE_VULKAN_HARDWARE_VERSION);
        Button create = actionButton(true);
        create.setText(R.string.new_project);
        create.setOnClickListener(view -> newProject());
        addAction(column, create);
        Button browse = actionButton(false);
        browse.setText(R.string.my_projects);
        browse.setOnClickListener(view -> listProjects());
        addAction(column, browse);
        create.setEnabled(vulkan);
        browse.setEnabled(vulkan);
        if (!vulkan) {
            create.setAlpha(0.45f);
            browse.setAlpha(0.45f);
        }
        if (returnToEditor) {
            Button back = actionButton(false);
            back.setText(R.string.return_to_editor);
            back.setOnClickListener(view -> finish());
            addAction(column, back);
        }
        updates = actionButton(false);
        updates.setOnClickListener(view -> updateAction());
        addAction(column, updates);

        Switch theme = new Switch(this);
        theme.setText(R.string.dark_mode);
        theme.setMinHeight(dp(48));
        theme.setPadding(dp(8), dp(8), dp(8), 0);
        theme.setChecked(dark);
        column.addView(theme, new LinearLayout.LayoutParams(-1, -2));
        theme.setOnCheckedChangeListener((button, checked) -> {
            if (checked == dark) return;
            try {
                if (!Projects.setDark(this, checked)) throw new IOException("Cannot change theme");
                recreate();
            } catch (IOException error) {
                button.setChecked(dark);
                status.setText(R.string.theme_error);
            }
        });

        status = new TextView(this);
        status.setGravity(Gravity.CENTER);
        status.setTextColor(muted());
        status.setPadding(0, dp(10), 0, dp(2));
        column.addView(status);

        if (!vulkan && !returnToEditor) {
            TextView warning = new TextView(this);
            warning.setText(R.string.no_vulkan);
            warning.setGravity(Gravity.CENTER);
            warning.setTextColor(muted());
            column.addView(warning);
        }
        check(onlyUpdates);
        if (vulkan && state == null) {
            if (getIntent().getBooleanExtra(AUTO_CREATE_PROJECT, false)) newProject();
            else if (getIntent().getBooleanExtra(AUTO_OPEN_PROJECTS, false)) listProjects();
        }
    }

    @Override
    public void onBackPressed() {
        if (getIntent().getBooleanExtra(PROJECT_CLOSED, false)) moveTaskToBack(true);
        else super.onBackPressed();
    }

    private int border() { return dark ? 0xFF36546C : BORDER; }

    private int muted() { return dark ? 0xFFAFCBDD : MUTED; }

    private void addAction(LinearLayout column, Button button) {
        LinearLayout.LayoutParams layout = new LinearLayout.LayoutParams(-1, -2);
        if (column.getChildCount() > 0) layout.topMargin = dp(8);
        column.addView(button, layout);
    }

    private void listProjects() {
        List<Projects.Project> entries = projects.list(getString(R.string.legacy_project));
        AlertDialog.Builder dialog = new AlertDialog.Builder(this)
                .setTitle(R.string.my_projects)
                .setNegativeButton(android.R.string.cancel, null);
        if (entries.isEmpty()) {
            dialog.setMessage(R.string.no_projects).setPositiveButton(R.string.new_project, (ignored, which) -> newProject());
        } else {
            CharSequence[] labels = new CharSequence[entries.size()];
            for (int index = 0; index < entries.size(); index++) {
                Projects.Project project = entries.get(index);
                labels[index] = project.name() + " · " + getString(project.modeling() ? R.string.project_model : R.string.project_house);
            }
            dialog.setItems(labels, (ignored, which) -> openProject(entries.get(which)));
        }
        dialog.show();
    }

    private void newProject() {
        LinearLayout form = new LinearLayout(this);
        form.setOrientation(LinearLayout.VERTICAL);
        form.setPadding(dp(22), dp(12), dp(22), 0);
        EditText name = new EditText(this);
        name.setHint(R.string.project_name);
        name.setSingleLine(true);
        name.setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_CAP_SENTENCES);
        name.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(80)});
        form.addView(name);
        android.widget.RadioGroup types = new android.widget.RadioGroup(this);
        android.widget.RadioButton house = new android.widget.RadioButton(this);
        house.setId(android.view.View.generateViewId());
        house.setText(R.string.project_house);
        types.addView(house);
        android.widget.RadioButton modeling = new android.widget.RadioButton(this);
        modeling.setId(android.view.View.generateViewId());
        modeling.setText(R.string.project_model);
        types.addView(modeling);
        types.check(house.getId());
        form.addView(types);
        new AlertDialog.Builder(this)
                .setTitle(R.string.new_project)
                .setView(form)
                .setNegativeButton(android.R.string.cancel, null)
                .setPositiveButton(R.string.create_project, (dialog, which) -> {
                    String title = name.getText().toString().trim();
                    if (title.isEmpty()) title = getString(R.string.untitled_project);
                    try {
                        openProject(projects.create(title, types.getCheckedRadioButtonId() == modeling.getId()));
                    } catch (IOException error) {
                        status.setText(R.string.project_error);
                    }
                }).show();
    }

    private void openProject(Projects.Project project) {
        startActivity(new Intent(this, PlannerActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(PROJECT_ID, project.id())
                .putExtra(PROJECT_NAME, project.name())
                .putExtra(PROJECT_MODEL, project.modeling()));
    }

    private Button actionButton(boolean primary) {
        Button button = new Button(this);
        button.setAllCaps(false);
        button.setTextSize(16);
        button.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        button.setTextColor(primary ? Color.WHITE : dark ? 0xFFB3E2FF : BLUE);
        button.setBackground(new RippleDrawable(
                ColorStateList.valueOf(primary ? 0x33FFFFFF : 0x26006C9F),
                surface(primary ? BLUE : dark ? 0xFF27445E : BLUE_SURFACE, primary ? BLUE : border(), 14), null));
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
