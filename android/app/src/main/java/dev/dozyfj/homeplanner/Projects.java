package dev.dozyfj.homeplanner;

import android.content.Context;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.UUID;

final class Projects {
    static final String LEGACY = "legacy";
    private static final String TITLE = "name.txt";
    private static final String MODEL_ONLY = "model-mode";

    record Project(String id, String name, boolean modeling, long changed) {}

    private final File files;

    Projects(Context context) {
        files = context.getFilesDir();
    }

    File directory(String id) {
        if (LEGACY.equals(id)) return files;
        if (!isId(id)) throw new IllegalArgumentException("Invalid project ID");
        return new File(new File(files, "projects"), id);
    }

    static boolean isId(String id) {
        return id != null && id.matches("[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}");
    }

    Project create(String name, boolean modeling) throws IOException {
        String id = UUID.randomUUID().toString();
        File dir = directory(id);
        if (!dir.mkdirs()) throw new IOException("Cannot create project directory");
        if (modeling && !new File(dir, MODEL_ONLY).createNewFile()) throw new IOException("Cannot set project type");
        Files.write(new File(dir, TITLE).toPath(), name.getBytes(StandardCharsets.UTF_8));
        return new Project(id, name, modeling, dir.lastModified());
    }

    List<Project> list(String oldName) {
        List<Project> result = new ArrayList<>();
        long legacy = changed(files);
        if (legacy > 0) {
            boolean modeling = !new File(files, "house.json").exists() && new File(files, "model.json").isFile();
            result.add(new Project(LEGACY, oldName, modeling, legacy));
        }
        File[] folders = new File(files, "projects").listFiles(File::isDirectory);
        if (folders != null) {
            for (File folder : folders) {
                String id = folder.getName();
                File nameFile = new File(folder, TITLE);
                if (!isId(id) || !nameFile.isFile() || nameFile.length() > 1024) continue;
                try {
                    String name = new String(Files.readAllBytes(nameFile.toPath()), StandardCharsets.UTF_8);
                    result.add(new Project(id, name, new File(folder, MODEL_ONLY).isFile(),
                            Math.max(changed(folder), nameFile.lastModified())));
                } catch (IOException ignored) {
                    // A project still being written can be loaded the next time the list opens.
                }
            }
        }
        result.sort(Comparator.comparingLong(Project::changed).reversed().thenComparing(Project::name));
        return result;
    }

    private static long changed(File folder) {
        return Math.max(new File(folder, "house.json").lastModified(),
                Math.max(new File(folder, "model.json").lastModified(), new File(folder, "plan-photo.gray").lastModified()));
    }

    static boolean dark(Context context) {
        return new File(context.getFilesDir(), "dark-mode").isFile();
    }

    static boolean setDark(Context context, boolean enabled) throws IOException {
        File flag = new File(context.getFilesDir(), "dark-mode");
        return enabled ? flag.isFile() || flag.createNewFile() : !flag.exists() || flag.delete();
    }
}
