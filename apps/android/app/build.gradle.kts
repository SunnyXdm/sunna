import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// The release key lives outside the repository: it signs every update, so
// keep a backup of it. A properties file names it (storeFile, storePassword,
// keyAlias, keyPassword): -Psunna.signing=<file> or SUNNA_ANDROID_SIGNING, by
// default ~/.sunna/android/signing.properties. Without one, release builds
// are signed with the debug key.
val signing: Properties? = run {
    val path = providers.gradleProperty("sunna.signing").orNull
        ?: System.getenv("SUNNA_ANDROID_SIGNING")
        ?: "${System.getProperty("user.home")}/.sunna/android/signing.properties"
    val file = File(path)
    if (file.isFile) Properties().apply { file.inputStream().use { load(it) } } else null
}

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.sunna.app"
    compileSdk = 36
    ndkVersion = "27.1.12297006"

    defaultConfig {
        applicationId = "dev.sunna.app"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.0.1"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    signingConfigs {
        if (signing != null) {
            create("release") {
                storeFile = File(signing.getProperty("storeFile"))
                storePassword = signing.getProperty("storePassword")
                keyAlias = signing.getProperty("keyAlias")
                keyPassword = signing.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rust/jniLibs"))
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

// The native side (crates/android), built for the phone's processors with
// cargo-ndk, always optimized: a debug build can't keep up with video.
// -Psunna.rustToolchain=1.98.1 picks a rustup toolchain.
val buildRust by tasks.registering(Exec::class) {
    val repo = rootDir.parentFile.parentFile
    val out = layout.buildDirectory.dir("rust/jniLibs").get().asFile
    workingDir = repo
    inputs.dir(File(repo, "crates"))
    inputs.file(File(repo, "Cargo.lock"))
    outputs.dir(out)
    val toolchain = providers.gradleProperty("sunna.rustToolchain").orNull
    if (toolchain != null) environment("RUSTUP_TOOLCHAIN", toolchain)
    environment("ANDROID_NDK_HOME", android.ndkDirectory.absolutePath)
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a", "-t", "x86_64",
        "--platform", "29",
        "-o", out.absolutePath,
        "build", "--release", "-p", "sunna-android",
    )
}

tasks.named("preBuild") {
    dependsOn(buildRust)
}
