// The APK of Jade Cocoon on Bevy. The game itself is the native library of
// `crates/jc-android`, which `build-apk.ps1` builds with cargo-ndk into `jniLibs/`
// before Gradle packs it; the Java here is only the first screen (the disc image) and
// the GameActivity the game runs in.
import java.util.Properties

plugins {
    id("com.android.application")
}

// The version is the workspace's (`Cargo.toml`), so the APK and the exe of a commit
// name the same one: 1.2.3 is version code 10203.
val repository: File = rootDir.parentFile
val workspaceVersion: String = Regex("""\[workspace\.package\][^\[]*?\bversion\s*=\s*"([^"]+)"""")
    .find(repository.resolve("Cargo.toml").readText())
    ?.groupValues?.get(1)
    ?: error("no version in [workspace.package] of Cargo.toml")
val versionParts: List<Int> = workspaceVersion.split(".").map { it.takeWhile(Char::isDigit).toInt() }

// The key of a release: `-PjcKeystore=path\to\keystore.properties`, a file with
// storeFile (beside it or absolute), storePassword, keyAlias and keyPassword. Without
// it the APK is signed with this PC's debug key, enough to install it by hand.
val keystoreFile: File? = (findProperty("jcKeystore") as String?)?.let { file(it) }

android {
    namespace = "io.github.ufodriverr.jadecocoon"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.ufodriverr.jadecocoon"
        minSdk = 28
        targetSdk = 36
        versionCode = versionParts[0] * 10000 + versionParts[1] * 100 + versionParts[2]
        versionName = workspaceVersion
        // A phone is arm64; `-PjcAbis=arm64-v8a,x86_64` adds the emulator's.
        ndk {
            val abis = (project.findProperty("jcAbis") as String?) ?: "arm64-v8a"
            abiFilters += abis.split(",").map { it.trim() }
        }
    }

    signingConfigs {
        if (keystoreFile != null) {
            create("release") {
                val keys = Properties().apply { keystoreFile.inputStream().use { load(it) } }
                fun key(name: String): String = keys.getProperty(name) ?: error("$name missing in $keystoreFile")
                storeFile = keystoreFile.parentFile.resolve(key("storeFile"))
                storePassword = key("storePassword")
                keyAlias = key("keyAlias")
                keyPassword = key("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName(if (keystoreFile != null) "release" else "debug")
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.directories.add("../jniLibs")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
            // cargo-ndk copies every library cargo made; the game is one of them alone.
            excludes += listOf("**/libiroh*.so")
        }
    }
}

// The licence and the notices travel inside the APK (`assets/legal/`), as they travel
// in the Windows ZIP beside the exe.
abstract class LegalNotices : DefaultTask() {
    @get:InputFiles
    abstract val notices: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @TaskAction
    fun copy() {
        val legal = outputDir.get().asFile.resolve("legal")
        legal.mkdirs()
        for (notice in notices.files) {
            when {
                notice.isFile -> notice.copyTo(legal.resolve(notice.name), overwrite = true)
                notice.isDirectory -> notice.copyRecursively(legal.resolve(notice.name), overwrite = true)
            }
        }
    }
}

val legalNotices = tasks.register<LegalNotices>("legalNotices") {
    notices.from(
        listOf("LICENSE", "LEGAL.md", "THIRD-PARTY-NOTICES.md", "licenses")
            .map { repository.resolve(it) }
            .filter { it.exists() },
    )
}

androidComponents {
    onVariants { variant ->
        variant.sources.assets?.addGeneratedSourceDirectory(legalNotices, LegalNotices::outputDir)
    }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.core:core:1.16.0")
    // The version android-activity 0.6 is made for (its README).
    implementation("androidx.games:games-activity:4.4.0")
}
