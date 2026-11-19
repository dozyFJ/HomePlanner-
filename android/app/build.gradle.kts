plugins {
    id("com.android.application")
}

val signingFile = System.getenv("HOMEPLANNER_KEYSTORE")
val signingPassword = System.getenv("HOMEPLANNER_KEYSTORE_PASSWORD")

android {
    namespace = "dev.dozyfj.homeplanner"
    compileSdk = 35
    defaultConfig {
        applicationId = "dev.dozyfj.homeplanner"
        minSdk = 26
        targetSdk = 35
        versionCode = 13
        versionName = "0.1.12"
        ndk { abiFilters += setOf("arm64-v8a") }
    }
    if (!signingFile.isNullOrBlank() && !signingPassword.isNullOrBlank()) {
        signingConfigs.create("ota") {
            storeFile = file(signingFile)
            storePassword = signingPassword
            keyAlias = "homeplanner"
            keyPassword = signingPassword
        }
        buildTypes.getByName("release").signingConfig = signingConfigs.getByName("ota")
    } else {
        buildTypes.getByName("release").signingConfig = signingConfigs.getByName("debug")
    }

    sourceSets.getByName("main").jniLibs.srcDir(layout.buildDirectory.dir("rust-libs"))

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.core:core:1.16.0")
}

tasks.named("preBuild") {
    doFirst {
        val libs = layout.buildDirectory.dir("rust-libs").get().asFile
        check(libs.resolve("arm64-v8a/libwolfcraft_home_planner.so").isFile) {
            "Run scripts/build-android.sh to compile and stage the native renderer first."
        }
    }
}
