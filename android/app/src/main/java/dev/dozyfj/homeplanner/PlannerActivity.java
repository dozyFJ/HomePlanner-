package dev.dozyfj.homeplanner;

import android.app.NativeActivity;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.ImageDecoder;
import android.net.Uri;
import android.os.Build;
import android.widget.Toast;

import java.io.DataOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;

public final class PlannerActivity extends NativeActivity {
    private static final int PICK_PLAN = 71;
    private static final int MAX_SIDE = 1024;

    public void showUpdates() {
        runOnUiThread(() -> startActivity(new Intent(this, LauncherActivity.class)
                .putExtra(LauncherActivity.ONLY_UPDATES, true)));
    }

    public void pickPlanPhoto() {
        runOnUiThread(() -> {
            Intent picker = new Intent(Intent.ACTION_OPEN_DOCUMENT)
                    .addCategory(Intent.CATEGORY_OPENABLE)
                    .setType("image/*");
            try {
                startActivityForResult(picker, PICK_PLAN);
            } catch (android.content.ActivityNotFoundException error) {
                Toast.makeText(this, "На устройстве нет выбора файлов", Toast.LENGTH_LONG).show();
            }
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != PICK_PLAN || resultCode != RESULT_OK || data == null || data.getData() == null) return;
        Uri uri = data.getData();
        new Thread(() -> {
            try {
                storePhoto(uri);
                runOnUiThread(() -> Toast.makeText(this, "Фото плана загружено", Toast.LENGTH_SHORT).show());
            } catch (Exception error) {
                runOnUiThread(() -> Toast.makeText(this, "Не удалось открыть фото плана", Toast.LENGTH_LONG).show());
            }
        }, "plan-photo-import").start();
    }

    private Bitmap decode(Uri uri) throws IOException {
        if (Build.VERSION.SDK_INT >= 28) {
            return ImageDecoder.decodeBitmap(ImageDecoder.createSource(getContentResolver(), uri),
                    (decoder, info, source) -> {
                        int width = info.getSize().getWidth();
                        int height = info.getSize().getHeight();
                        float factor = Math.min(1f, (float) MAX_SIDE / Math.max(width, height));
                        decoder.setTargetSize(Math.max(1, Math.round(width * factor)), Math.max(1, Math.round(height * factor)));
                        decoder.setAllocator(ImageDecoder.ALLOCATOR_SOFTWARE);
                    });
        }
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inJustDecodeBounds = true;
        try (InputStream input = getContentResolver().openInputStream(uri)) {
            BitmapFactory.decodeStream(input, null, options);
        }
        if (options.outWidth <= 0 || options.outHeight <= 0) throw new IOException("Invalid image");
        options.inJustDecodeBounds = false;
        options.inSampleSize = 1;
        while (Math.max(options.outWidth, options.outHeight) / options.inSampleSize > MAX_SIDE) options.inSampleSize *= 2;
        try (InputStream input = getContentResolver().openInputStream(uri)) {
            Bitmap bitmap = BitmapFactory.decodeStream(input, null, options);
            if (bitmap == null) throw new IOException("Invalid image");
            return bitmap;
        }
    }

    private void storePhoto(Uri uri) throws IOException {
        Bitmap bitmap = decode(uri);
        File partial = File.createTempFile("plan-photo-", ".part", getFilesDir());
        try {
            try (DataOutputStream out = new DataOutputStream(new FileOutputStream(partial))) {
                int width = bitmap.getWidth();
                int height = bitmap.getHeight();
                out.writeInt(width);
                out.writeInt(height);
                int[] row = new int[width];
                byte[] gray = new byte[width];
                for (int y = 0; y < height; y++) {
                    bitmap.getPixels(row, 0, width, 0, y, width, 1);
                    for (int x = 0; x < width; x++) {
                        int color = row[x];
                        int value = (54 * ((color >>> 16) & 255) + 183 * ((color >>> 8) & 255) + 19 * (color & 255)) >>> 8;
                        int alpha = color >>> 24;
                        gray[x] = (byte) ((value * alpha + 255 * (255 - alpha)) / 255);
                    }
                    out.write(gray);
                }
            }
            if (!partial.renameTo(new File(getFilesDir(), "plan-photo.gray"))) throw new IOException("Cannot replace photo");
        } finally {
            partial.delete();
            bitmap.recycle();
        }
    }
}
