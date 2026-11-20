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
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.Switch;
import android.widget.TextView;

import androidx.core.content.FileProvider;

import java.io.File;
import java.io.IOException;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.List;
import java.util.Locale;
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
    private Button updatesBtn;
    private boolean dark;
    private Projects projects;

    private int activeTab = 0; // 0 = Последние, 1 = Все проекты
    private LinearLayout projectsContainer;
    private Button tabRecent;
    private Button tabAll;

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
        page.setGravity(Gravity.TOP | Gravity.CENTER_HORIZONTAL);
        page.setPadding(dp(20), dp(24), dp(20), dp(24));
        scroll.addView(page);
        setContentView(scroll);

        // --- ВЕРХНЯЯ ШАПКА ---
        LinearLayout heading = new LinearLayout(this);
        heading.setOrientation(LinearLayout.HORIZONTAL);
        heading.setGravity(Gravity.CENTER_VERTICAL);
        heading.setPadding(dp(18), dp(16), dp(16), dp(16));
        GradientDrawable headerBg = new GradientDrawable(GradientDrawable.Orientation.TL_BR,
                new int[]{0xFF005986, 0xFF087FB7});
        headerBg.setCornerRadius(dp(22));
        heading.setBackground(headerBg);
        page.addView(heading, new LinearLayout.LayoutParams(-1, -2));

        // Иконка логотипа дома
        ImageView logoIcon = new ImageView(this);
        logoIcon.setImageDrawable(createHomeLogoDrawable());
        int iconSize = dp(42);
        LinearLayout.LayoutParams iconParams = new LinearLayout.LayoutParams(iconSize, iconSize);
        iconParams.rightMargin = dp(14);
        heading.addView(logoIcon, iconParams);

        // Заголовки (Название + Версия)
        LinearLayout titleCol = new LinearLayout(this);
        titleCol.setOrientation(LinearLayout.VERTICAL);
        LinearLayout.LayoutParams titleColParams = new LinearLayout.LayoutParams(0, -2, 1.0f);
        heading.addView(titleCol, titleColParams);

        TextView title = new TextView(this);
        title.setText(onlyUpdates ? R.string.update_title : R.string.app_name);
        title.setTextColor(Color.WHITE);
        title.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        title.setTextSize(18);
        titleCol.addView(title);

        TextView version = new TextView(this);
        PackageInfo installed = client.installed();
        version.setText(getString(R.string.current_version, installed.versionName));
        version.setTextColor(0xFFD6F0FF);
        version.setTextSize(13);
        version.setPadding(0, dp(2), 0, 0);
        titleCol.addView(version);

        // Кнопка обновления в шапке
        updatesBtn = new Button(this);
        updatesBtn.setText(R.string.updates_btn);
        updatesBtn.setTextSize(12);
        updatesBtn.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        updatesBtn.setTextColor(Color.WHITE);
        updatesBtn.setAllCaps(false);
        updatesBtn.setBackground(new RippleDrawable(
                ColorStateList.valueOf(0x44FFFFFF),
                surface(0x33FFFFFF, 0x66FFFFFF, 12), null));
        updatesBtn.setMinHeight(dp(36));
        updatesBtn.setMinimumHeight(dp(36));
        updatesBtn.setPadding(dp(12), dp(4), dp(12), dp(4));
        updatesBtn.setOnClickListener(v -> updateAction());
        heading.addView(updatesBtn, new LinearLayout.LayoutParams(-2, dp(38)));

        // --- КНОПКИ ДЕЙСТВИЯ (Новый проект / Вернуться) ---
        boolean vulkan = getPackageManager().hasSystemFeature(PackageManager.FEATURE_VULKAN_HARDWARE_VERSION);

        LinearLayout primaryActions = new LinearLayout(this);
        primaryActions.setOrientation(LinearLayout.VERTICAL);
        LinearLayout.LayoutParams primaryParams = new LinearLayout.LayoutParams(-1, -2);
        primaryParams.topMargin = dp(14);
        page.addView(primaryActions, primaryParams);

        Button create = actionButton(true);
        create.setText(R.string.new_project);
        create.setOnClickListener(view -> newProject());
        primaryActions.addView(create);

        create.setEnabled(vulkan);
        if (!vulkan) create.setAlpha(0.45f);

        if (returnToEditor) {
            Button back = actionButton(false);
            back.setText(R.string.return_to_editor);
            back.setOnClickListener(view -> finish());
            LinearLayout.LayoutParams backParams = new LinearLayout.LayoutParams(-1, -2);
            backParams.topMargin = dp(8);
            primaryActions.addView(back, backParams);
        }

        // --- БЛОК ПРОЕКТОВ (ТАБЫ: ПОСЛЕДНИЕ / ВСЕ ПРОЕКТЫ) ---
        LinearLayout projectsCard = new LinearLayout(this);
        projectsCard.setOrientation(LinearLayout.VERTICAL);
        projectsCard.setPadding(dp(14), dp(14), dp(14), dp(14));
        projectsCard.setBackground(surface(dark ? 0xFF1E3143 : Color.WHITE, border(), 20));
        LinearLayout.LayoutParams cardParams = new LinearLayout.LayoutParams(-1, -2);
        cardParams.topMargin = dp(14);
        page.addView(projectsCard, cardParams);

        // Таб-бар
        LinearLayout tabRow = new LinearLayout(this);
        tabRow.setOrientation(LinearLayout.HORIZONTAL);
        tabRow.setBackground(surface(dark ? 0xFF14222F : 0xFFEEF5FA, border(), 12));
        tabRow.setPadding(dp(3), dp(3), dp(3), dp(3));
        projectsCard.addView(tabRow, new LinearLayout.LayoutParams(-1, dp(40)));

        tabRecent = createTabButton(getString(R.string.recent_projects), true);
        tabRecent.setOnClickListener(v -> selectTab(0));
        tabRow.addView(tabRecent, new LinearLayout.LayoutParams(0, -1, 1.0f));

        tabAll = createTabButton(getString(R.string.all_projects), false);
        tabAll.setOnClickListener(v -> selectTab(1));
        tabRow.addView(tabAll, new LinearLayout.LayoutParams(0, -1, 1.0f));

        // Контейнер списка проектов
        projectsContainer = new LinearLayout(this);
        projectsContainer.setOrientation(LinearLayout.VERTICAL);
        projectsContainer.setPadding(0, dp(10), 0, 0);
        projectsCard.addView(projectsContainer, new LinearLayout.LayoutParams(-1, -2));

        // Статус обновлений
        status = new TextView(this);
        status.setGravity(Gravity.CENTER);
        status.setTextColor(muted());
        status.setTextSize(13);
        status.setPadding(0, dp(8), 0, dp(2));
        projectsCard.addView(status);

        // --- НИЖНЯЯ ПАНЕЛЬ С ТЕМОЙ ---
        LinearLayout footer = new LinearLayout(this);
        footer.setOrientation(LinearLayout.HORIZONTAL);
        footer.setGravity(Gravity.CENTER_VERTICAL);
        footer.setPadding(dp(16), dp(10), dp(16), dp(10));
        footer.setBackground(surface(dark ? 0xFF1E3143 : Color.WHITE, border(), 16));
        LinearLayout.LayoutParams footerParams = new LinearLayout.LayoutParams(-1, -2);
        footerParams.topMargin = dp(12);
        page.addView(footer, footerParams);

        TextView themeLabel = new TextView(this);
        themeLabel.setText(R.string.dark_mode);
        themeLabel.setTextColor(dark ? 0xFFE0EEF8 : 0xFF1A3347);
        themeLabel.setTextSize(15);
        footer.addView(themeLabel, new LinearLayout.LayoutParams(0, -2, 1.0f));

        Switch theme = new Switch(this);
        theme.setChecked(dark);
        footer.addView(theme);
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

        if (!vulkan && !returnToEditor) {
            TextView warning = new TextView(this);
            warning.setText(R.string.no_vulkan);
            warning.setGravity(Gravity.CENTER);
            warning.setTextColor(muted());
            warning.setPadding(0, dp(10), 0, 0);
            page.addView(warning);
        }

        renderProjects();
        check(onlyUpdates);

        if (vulkan && state == null) {
            if (getIntent().getBooleanExtra(AUTO_CREATE_PROJECT, false)) newProject();
            else if (getIntent().getBooleanExtra(AUTO_OPEN_PROJECTS, false)) listProjects();
        }
    }

    private Button createTabButton(String text, boolean active) {
        Button btn = new Button(this);
        btn.setText(text);
        btn.setTextSize(13);
        btn.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        btn.setAllCaps(false);
        btn.setMinHeight(0);
        btn.setMinimumHeight(0);
        applyTabStyle(btn, active);
        return btn;
    }

    private void applyTabStyle(Button btn, boolean active) {
        if (active) {
            btn.setTextColor(Color.WHITE);
            btn.setBackground(surface(BLUE, BLUE, 10));
        } else {
            btn.setTextColor(dark ? 0xFF8AAEC8 : 0xFF50718A);
            btn.setBackground(new GradientDrawable());
        }
    }

    private void selectTab(int tab) {
        if (activeTab == tab) return;
        activeTab = tab;
        applyTabStyle(tabRecent, activeTab == 0);
        applyTabStyle(tabAll, activeTab == 1);
        renderProjects();
    }

    private void renderProjects() {
        projectsContainer.removeAllViews();
        List<Projects.Project> list = projects.list(getString(R.string.legacy_project));

        if (list.isEmpty()) {
            TextView empty = new TextView(this);
            empty.setText(R.string.no_projects);
            empty.setTextColor(muted());
            empty.setGravity(Gravity.CENTER);
            empty.setPadding(dp(12), dp(20), dp(12), dp(20));
            projectsContainer.addView(empty);
            return;
        }

        int count = (activeTab == 0) ? Math.min(list.size(), 3) : list.size();
        SimpleDateFormat dateFormat = new SimpleDateFormat("d MMM, HH:mm", Locale.getDefault());

        for (int i = 0; i < count; i++) {
            Projects.Project project = list.get(i);

            LinearLayout item = new LinearLayout(this);
            item.setOrientation(LinearLayout.HORIZONTAL);
            item.setGravity(Gravity.CENTER_VERTICAL);
            item.setPadding(dp(12), dp(10), dp(12), dp(10));
            item.setBackground(new RippleDrawable(
                    ColorStateList.valueOf(0x20006C9F),
                    surface(dark ? 0xFF243B52 : 0xFFF5FAFD, border(), 12), null));
            item.setClickable(true);
            item.setFocusable(true);
            item.setOnClickListener(v -> openProject(project));

            // Иконка типа проекта
            ImageView icon = new ImageView(this);
            icon.setImageDrawable(createProjectIconDrawable(project.modeling()));
            int icSize = dp(28);
            LinearLayout.LayoutParams icParams = new LinearLayout.LayoutParams(icSize, icSize);
            icParams.rightMargin = dp(10);
            item.addView(icon, icParams);

            LinearLayout info = new LinearLayout(this);
            info.setOrientation(LinearLayout.VERTICAL);
            LinearLayout.LayoutParams infoParams = new LinearLayout.LayoutParams(0, -2, 1.0f);
            item.addView(info, infoParams);

            TextView nameView = new TextView(this);
            nameView.setText(project.name());
            nameView.setTextColor(dark ? Color.WHITE : 0xFF142838);
            nameView.setTextSize(15);
            nameView.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
            info.addView(nameView);

            TextView descView = new TextView(this);
            String typeStr = getString(project.modeling() ? R.string.project_model : R.string.project_house);
            String dateStr = dateFormat.format(new Date(project.changed()));
            descView.setText(typeStr + " · " + dateStr);
            descView.setTextColor(muted());
            descView.setTextSize(12);
            info.addView(descView);

            TextView arrow = new TextView(this);
            arrow.setText("›");
            arrow.setTextSize(20);
            arrow.setTextColor(muted());
            arrow.setPadding(dp(6), 0, 0, 0);
            item.addView(arrow);

            LinearLayout.LayoutParams rowParams = new LinearLayout.LayoutParams(-1, -2);
            rowParams.topMargin = dp(6);
            projectsContainer.addView(item, rowParams);
        }
    }

    private android.graphics.drawable.Drawable createHomeLogoDrawable() {
        return new android.graphics.drawable.Drawable() {
            private final android.graphics.Paint paint = new android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG);

            @Override
            public void draw(android.graphics.Canvas canvas) {
                android.graphics.Rect b = getBounds();
                float w = b.width();
                float h = b.height();
                float left = b.left;
                float top = b.top;

                // Фон плашки логотипа
                paint.setColor(0x33FFFFFF);
                paint.setStyle(android.graphics.Paint.Style.FILL);
                canvas.drawRoundRect(left, top, left + w, top + h, dp(10), dp(10), paint);

                // Контур дома
                paint.setColor(Color.WHITE);
                paint.setStyle(android.graphics.Paint.Style.STROKE);
                paint.setStrokeWidth(dp(2));
                paint.setStrokeCap(android.graphics.Paint.Cap.ROUND);
                paint.setStrokeJoin(android.graphics.Paint.Join.ROUND);

                android.graphics.Path path = new android.graphics.Path();
                // Крыша
                path.moveTo(left + w * 0.20f, top + h * 0.48f);
                path.lineTo(left + w * 0.50f, top + h * 0.24f);
                path.lineTo(left + w * 0.80f, top + h * 0.48f);
                // Стены
                path.moveTo(left + w * 0.30f, top + h * 0.48f);
                path.lineTo(left + w * 0.30f, top + h * 0.76f);
                path.lineTo(left + w * 0.70f, top + h * 0.76f);
                path.lineTo(left + w * 0.70f, top + h * 0.48f);
                // Дверь
                path.moveTo(left + w * 0.43f, top + h * 0.76f);
                path.lineTo(left + w * 0.43f, top + h * 0.58f);
                path.lineTo(left + w * 0.57f, top + h * 0.58f);
                path.lineTo(left + w * 0.57f, top + h * 0.76f);
                canvas.drawPath(path, paint);
            }

            @Override public void setAlpha(int alpha) { paint.setAlpha(alpha); }
            @Override public void setColorFilter(android.graphics.ColorFilter cf) { paint.setColorFilter(cf); }
            @Override public int getOpacity() { return android.graphics.PixelFormat.TRANSLUCENT; }
        };
    }

    private android.graphics.drawable.Drawable createProjectIconDrawable(boolean is3D) {
        return new android.graphics.drawable.Drawable() {
            private final android.graphics.Paint paint = new android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG);

            @Override
            public void draw(android.graphics.Canvas canvas) {
                android.graphics.Rect b = getBounds();
                float w = b.width();
                float h = b.height();
                float l = b.left;
                float t = b.top;

                paint.setColor(is3D ? (dark ? 0xFF356184 : 0xFFD8EEFA) : (dark ? 0xFF2A4C68 : 0xFFE3F2FD));
                paint.setStyle(android.graphics.Paint.Style.FILL);
                canvas.drawRoundRect(l, t, l + w, t + h, dp(8), dp(8), paint);

                paint.setColor(is3D ? 0xFF1488C8 : BLUE);
                paint.setStyle(android.graphics.Paint.Style.STROKE);
                paint.setStrokeWidth(dp(1.7f));
                paint.setStrokeCap(android.graphics.Paint.Cap.ROUND);
                paint.setStrokeJoin(android.graphics.Paint.Join.ROUND);

                if (is3D) {
                    // Иконка 3D куба
                    android.graphics.Path path = new android.graphics.Path();
                    path.moveTo(l + w * 0.5f, t + h * 0.22f);
                    path.lineTo(l + w * 0.8f, t + h * 0.38f);
                    path.lineTo(l + w * 0.8f, t + h * 0.72f);
                    path.lineTo(l + w * 0.5f, t + h * 0.88f);
                    path.lineTo(l + w * 0.2f, t + h * 0.72f);
                    path.lineTo(l + w * 0.2f, t + h * 0.38f);
                    path.close();
                    path.moveTo(l + w * 0.5f, t + h * 0.22f);
                    path.lineTo(l + w * 0.5f, t + h * 0.55f);
                    path.lineTo(l + w * 0.8f, t + h * 0.38f);
                    path.moveTo(l + w * 0.5f, t + h * 0.55f);
                    path.lineTo(l + w * 0.2f, t + h * 0.38f);
                    canvas.drawPath(path, paint);
                } else {
                    // Иконка чертежа / плана 2D
                    android.graphics.Path path = new android.graphics.Path();
                    path.addRoundRect(l + w * 0.22f, t + h * 0.22f, l + w * 0.78f, t + h * 0.78f, dp(4), dp(4), android.graphics.Path.Direction.CW);
                    path.moveTo(l + w * 0.48f, t + h * 0.22f);
                    path.lineTo(l + w * 0.48f, t + h * 0.78f);
                    path.moveTo(l + w * 0.22f, t + h * 0.50f);
                    path.lineTo(l + w * 0.48f, t + h * 0.50f);
                    canvas.drawPath(path, paint);
                }
            }

            @Override public void setAlpha(int alpha) { paint.setAlpha(alpha); }
            @Override public void setColorFilter(android.graphics.ColorFilter cf) { paint.setColorFilter(cf); }
            @Override public int getOpacity() { return android.graphics.PixelFormat.TRANSLUCENT; }
        };
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

    private float dp(float pixels) {
        return pixels * getResources().getDisplayMetrics().density;
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
        if (updatesBtn == null) return;
        if (downloaded != null) {
            updatesBtn.setText(getString(R.string.install_update, available.name()));
        } else if (available != null) {
            updatesBtn.setText(getString(R.string.download_update, available.name()));
        } else {
            updatesBtn.setText(R.string.updates_btn);
        }
        updatesBtn.setEnabled(enabled);
        updatesBtn.setAlpha(enabled ? 1f : 0.65f);
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
