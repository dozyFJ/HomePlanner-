package dev.dozyfj.homeplanner;

import android.content.Context;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.pm.Signature;
import android.os.Build;

import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Arrays;
import java.util.HashSet;
import java.util.Locale;
import java.util.regex.Pattern;

import javax.net.ssl.HttpsURLConnection;

final class UpdateClient {
    private static final String FEED = "https://raw.githubusercontent.com/dozyFJ/HomePlanner-/ota/";
    private static final Pattern APK_URL = Pattern.compile(Pattern.quote(FEED) + "releases/HomePlanner-[0-9.]+\\.apk");
    private static final Pattern HASH = Pattern.compile("[0-9a-f]{64}");
    private static final int MANIFEST_LIMIT = 16 * 1024;
    private static final int APK_LIMIT = 100 * 1024 * 1024;

    record Update(long code, String name, String url, String hash) {}

    private final Context context;
    private final PackageManager packages;

    UpdateClient(Context context) {
        this.context = context.getApplicationContext();
        packages = context.getPackageManager();
    }

    PackageInfo installed() {
        try {
            return packages.getPackageInfo(context.getPackageName(), signingFlag());
        } catch (PackageManager.NameNotFoundException error) {
            throw new IllegalStateException(error);
        }
    }

    Update check() throws Exception {
        HttpsURLConnection connection = connect(FEED + "latest.json");
        JSONObject json;
        try (InputStream input = connection.getInputStream()) {
            ByteArrayOutputStream buffer = new ByteArrayOutputStream();
            transfer(input, buffer, null, MANIFEST_LIMIT);
            json = new JSONObject(new String(buffer.toByteArray(), StandardCharsets.UTF_8));
        } finally {
            connection.disconnect();
        }
        long code = json.getLong("versionCode");
        String name = json.getString("versionName");
        String url = json.getString("url");
        String hash = json.getString("sha256").toLowerCase(Locale.ROOT);
        if (code <= 0 || name.trim().isEmpty() || !APK_URL.matcher(url).matches() || !HASH.matcher(hash).matches()) {
            throw new IOException("Invalid release metadata");
        }
        if (code <= versionCode(installed())) return null;
        return new Update(code, name, url, hash);
    }

    File download(Update release) throws Exception {
        File directory = new File(context.getCacheDir(), "updates");
        if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("No APK cache");
        File partial = File.createTempFile("download-", ".apk", directory);
        File destination = new File(directory, "HomePlanner-" + release.code() + ".apk");
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            HttpsURLConnection connection = connect(release.url());
            try (InputStream input = connection.getInputStream();
                 FileOutputStream output = new FileOutputStream(partial)) {
                transfer(input, output, digest, APK_LIMIT);
            } finally {
                connection.disconnect();
            }
            StringBuilder hex = new StringBuilder(64);
            for (byte value : digest.digest()) hex.append(String.format(Locale.ROOT, "%02x", value & 0xff));
            if (!release.hash().contentEquals(hex)) throw new IOException("APK checksum mismatch");

            PackageInfo archive = packages.getPackageArchiveInfo(partial.getPath(), signingFlag());
            if (archive == null || !context.getPackageName().equals(archive.packageName)
                    || versionCode(archive) != release.code()
                    || !sameSigners(archive, installed())) {
                throw new IOException("APK identity, version or signature mismatch");
            }
            if (!partial.renameTo(destination)) throw new IOException("Could not save verified APK");
            return destination;
        } finally {
            partial.delete();
        }
    }

    private static void transfer(InputStream input, java.io.OutputStream output, MessageDigest digest, int limit)
            throws IOException {
        byte[] buffer = new byte[16 * 1024];
        int total = 0;
        int count;
        while ((count = input.read(buffer)) != -1) {
            if (Thread.currentThread().isInterrupted()) throw new IOException("Transfer interrupted");
            total += count;
            if (total > limit) throw new IOException("Response exceeds size limit");
            output.write(buffer, 0, count);
            if (digest != null) digest.update(buffer, 0, count);
        }
    }

    private static HttpsURLConnection connect(String url) throws IOException {
        HttpsURLConnection connection = (HttpsURLConnection) new URL(url).openConnection();
        connection.setInstanceFollowRedirects(false);
        connection.setConnectTimeout(10_000);
        connection.setReadTimeout(15_000);
        try {
            if (connection.getResponseCode() != 200) throw new IOException("Update request failed");
            return connection;
        } catch (IOException error) {
            connection.disconnect();
            throw error;
        }
    }

    @SuppressWarnings("deprecation")
    private static int signingFlag() {
        return Build.VERSION.SDK_INT >= 28
                ? PackageManager.GET_SIGNING_CERTIFICATES : PackageManager.GET_SIGNATURES;
    }

    @SuppressWarnings("deprecation")
    private static long versionCode(PackageInfo info) {
        return Build.VERSION.SDK_INT >= 28 ? info.getLongVersionCode() : info.versionCode;
    }

    @SuppressWarnings("deprecation")
    private static Signature[] signers(PackageInfo info) {
        if (Build.VERSION.SDK_INT < 28) return info.signatures;
        return info.signingInfo == null ? null : info.signingInfo.getApkContentsSigners();
    }

    private static boolean sameSigners(PackageInfo a, PackageInfo b) {
        Signature[] left = signers(a);
        Signature[] right = signers(b);
        return left != null && right != null && left.length > 0
                && new HashSet<>(Arrays.asList(left)).equals(new HashSet<>(Arrays.asList(right)));
    }
}
