//! Android: библиотека через cargo-ndk, gradle-проект генерируется из метаданных в
//! target/syngui/android/<крейт> (GameActivity, FileProvider syngui, значки из SVG),
//! APK выравнивается и подписывается apksigner: релизным ключом из ~/.secrets/<крейт>-android,
//! если он есть (тогда и сборки себе ставятся поверх релиза), иначе отладочным.

use crate::icon;
use crate::project::{syngui_dir, App};
use crate::target::Profile;
use crate::util;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Версия GameActivity обязана совпадать с вшитой в android-activity (0.6.1 → 4.4.0),
/// иначе RegisterNatives падает на старте.
const GAMES_ACTIVITY: &str = "4.4.0";
const AGP: &str = "8.7.3";
const COMPILE_SDK: u32 = 35;
const ACTIVITY: &str = "com.google.androidgamesdk.GameActivity";

const GRADLEW: &[u8] = include_bytes!("../templates/android/gradlew");
const WRAPPER_JAR: &[u8] = include_bytes!("../templates/android/gradle/wrapper/gradle-wrapper.jar");
const WRAPPER_PROPS: &str = include_str!("../templates/android/gradle/wrapper/gradle-wrapper.properties");
const FOREGROUND_SERVICE: &str = include_str!("../../syngui/src/app/android/SynGuiForegroundService.java");

pub fn sdk_dir() -> PathBuf {
    std::env::var_os("ANDROID_HOME")
        .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
        .map(PathBuf::from)
        .unwrap_or_else(|| util::home().join("Android/Sdk"))
}

pub fn ndk_dir() -> Option<PathBuf> {
    for v in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME"] {
        if let Some(p) = std::env::var_os(v) {
            return Some(PathBuf::from(p));
        }
    }
    if Path::new("/opt/android-ndk/source.properties").is_file() {
        return Some("/opt/android-ndk".into());
    }
    util::latest_subdir(&sdk_dir().join("ndk"))
}

pub fn java_home() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("JAVA_HOME") {
        return Some(PathBuf::from(p));
    }
    // AGP 8.7 / gradle 8.11 работают с JDK 17–21; новее — не всегда
    ["/usr/lib/jvm/java-21-openjdk", "/usr/lib/jvm/java-17-openjdk"].iter().map(PathBuf::from).find(|p| p.join("bin/java").is_file())
}

/// Инструмент build-tools (apksigner, zipalign, aapt2): из PATH или последних build-tools SDK.
pub fn build_tool(name: &str) -> Option<PathBuf> {
    util::latest_subdir(&sdk_dir().join("build-tools")).map(|d| d.join(name)).filter(|p| p.is_file()).or_else(|| util::which(name))
}

pub fn abi_triple(abi: &str) -> Result<&'static str> {
    Ok(match abi {
        "arm64-v8a" => "aarch64-linux-android",
        "armeabi-v7a" => "armv7-linux-androideabi",
        "x86_64" => "x86_64-linux-android",
        "x86" => "i686-linux-android",
        _ => bail!("неизвестный ABI {abi}: arm64-v8a, armeabi-v7a, x86_64, x86"),
    })
}

fn env(c: &mut Command) {
    let sdk = sdk_dir();
    c.env("ANDROID_HOME", &sdk).env("ANDROID_SDK_ROOT", &sdk);
    if let Some(ndk) = ndk_dir() {
        c.env("ANDROID_NDK_HOME", &ndk).env("ANDROID_NDK_ROOT", &ndk);
    }
    if let Some(j) = java_home() {
        c.env("JAVA_HOME", j);
    }
}

pub struct Signing {
    pub keystore: PathBuf,
    pub password_file: PathBuf,
    pub alias: String,
    pub release: bool,
}

/// Каталог ключа приложения: $SYNGUI_ANDROID_KEYSTORE, [android] keystore или ~/.secrets/<крейт>-android.
pub fn key_dir(app: &App) -> PathBuf {
    if let Some(d) = std::env::var_os("SYNGUI_ANDROID_KEYSTORE") {
        return PathBuf::from(d);
    }
    app.meta.android.keystore.as_deref().map(util::expand).unwrap_or_else(|| util::home().join(".secrets").join(format!("{}-android", app.krate)))
}

/// Ключ подписи: релизный, если есть; для публикации — обязателен.
pub fn signing(app: &App, publish: bool) -> Result<Signing> {
    let dir = key_dir(app);
    let (ks, pw) = (dir.join("release.jks"), dir.join("password"));
    if ks.is_file() && pw.is_file() {
        return Ok(Signing { keystore: ks, password_file: pw, alias: app.meta.android.key_alias.clone().unwrap_or_else(|| app.krate.clone()), release: true });
    }
    if publish {
        bail!("нет ключа подписи {}/{{release.jks,password}} — создайте: syngui keygen -p {}", dir.display(), app.krate);
    }
    let dbg = util::home().join(".android/debug.keystore");
    if !dbg.is_file() {
        // keytool создаёт отладочный ключ так же, как Android Studio
        let mut c = Command::new("keytool");
        c.args(["-genkeypair", "-keystore"]).arg(&dbg).args([
            "-storepass", "android", "-keypass", "android", "-alias", "androiddebugkey", "-keyalg", "RSA", "-keysize", "2048", "-validity", "10000",
            "-dname", "CN=Android Debug,O=Android,C=US",
        ]);
        std::fs::create_dir_all(dbg.parent().unwrap())?;
        util::run(c).context("отладочный ключ ~/.android/debug.keystore")?;
    }
    let pwf = app.work_dir("android").join("debug-password");
    util::write_if_changed(&pwf, "android")?;
    Ok(Signing { keystore: dbg, password_file: pwf, alias: "androiddebugkey".into(), release: false })
}

/// Собрать подписанный APK; вернуть путь.
/// `abis` — вместо ABI из метаданных (сборка под подключённое устройство).
pub fn build(app: &App, profile: Profile, publish: bool, abis: Option<Vec<String>>) -> Result<PathBuf> {
    let a = &app.meta.android;
    let lib = a.lib.clone().or_else(|| app.lib.clone()).with_context(|| format!("у крейта {} нет [lib] (cdylib) для Android", app.krate))?;
    let abis = abis.or_else(|| a.abis.clone()).unwrap_or_else(|| vec!["arm64-v8a".into()]);
    let min_sdk = a.min_sdk.unwrap_or(30);
    let sign = signing(app, publish)?;

    let (gradle_dir, generated) = match &a.gradle_project {
        Some(p) => (app.path(p), false),
        None => (app.work_dir("android"), true),
    };
    if generated {
        generate(app, &gradle_dir, &lib, &abis)?;
    } else if !gradle_dir.join("local.properties").is_file() {
        util::write(&gradle_dir.join("local.properties"), format!("sdk.dir={}\n", sdk_dir().display()))?;
    }
    let jni = gradle_dir.join("app/src/main/jniLibs");

    // Rust → lib<name>.so для каждого ABI
    for abi in &abis {
        abi_triple(abi)?;
        util::step(format!("{}: lib{lib}.so ({abi}, {})", app.krate, profile.dir()));
        // cargo-ndk не копирует .so, если в jniLibs файл новее (прошлая сборка другим профилем) —
        // в APK попала бы чужая библиотека
        let _ = std::fs::remove_file(jni.join(abi).join(format!("lib{lib}.so")));
        let mut c = Command::new("cargo");
        c.current_dir(&app.manifest_dir).args(["ndk", "-t", abi, "-P", &min_sdk.to_string(), "-o"]).arg(&jni).arg("build");
        c.args(crate::linux::profile_args(app, profile));
        c.arg("--lib");
        if !a.default_features {
            c.arg("--no-default-features");
        }
        let features = a.features.clone().unwrap_or_else(|| vec!["android".into()]);
        if !features.is_empty() {
            c.arg("--features").arg(features.join(","));
        }
        // Статический FFmpeg под ABI: нужен, если включён syngui/ffmpeg-static (или android.ffmpeg = true)
        let need_ff = a.ffmpeg || wants_ffmpeg_static(app, &features, a.default_features);
        let ff = std::env::var_os("FFMPEG_DIR").map(PathBuf::from).unwrap_or_else(|| syngui_dir().join("android-deps/ffmpeg").join(abi));
        if ff.join("lib/libavcodec.a").is_file() {
            c.env("FFMPEG_DIR", &ff);
        } else if need_ff {
            bail!("нет статического FFmpeg для {abi}: {} — соберите: {}/scripts/build-ffmpeg-android.sh {abi} (или FFMPEG_DIR=<префикс>)", ff.display(), syngui_dir().display());
        }
        env(&mut c);
        util::run(c)?;
    }
    // Символы: cargo-ndk копирует .so как есть (сотни МБ с отладкой зависимостей)
    if a.strip.unwrap_or(profile != Profile::Dev) {
        let strip = ndk_dir()
            .map(|n| n.join("toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip"))
            .filter(|p| p.is_file())
            .or_else(|| util::which("llvm-strip"))
            .context("нет llvm-strip (NDK или пакет llvm) — или [package.metadata.syngui.android] strip = false")?;
        for abi in &abis {
            let so = jni.join(abi).join(format!("lib{lib}.so"));
            if !so.is_file() {
                bail!("cargo-ndk не оставил {}", so.display());
            }
            let before = std::fs::metadata(&so).map(|m| m.len()).unwrap_or(0);
            let mut c = Command::new(&strip);
            c.arg("--strip-unneeded").arg(&so);
            util::run(c)?;
            let after = std::fs::metadata(&so).map(|m| m.len()).unwrap_or(0);
            util::note(format!("strip lib{lib}.so ({abi}): {:.1} → {:.1} МБ", before as f64 / 1048576.0, after as f64 / 1048576.0));
        }
    }
    // Лишние ABI от прошлых сборок не тащить в APK
    if let Ok(rd) = std::fs::read_dir(&jni) {
        for e in rd.filter_map(|e| e.ok()) {
            if !abis.iter().any(|a| *a == e.file_name().to_string_lossy()) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    util::step(format!("{}: APK (gradle)", app.krate));
    let mut c = Command::new(gradle_dir.join("gradlew"));
    c.current_dir(&gradle_dir).args(["--quiet", "assembleRelease"]);
    env(&mut c);
    util::run(c)?;
    let outs = gradle_dir.join("app/build/outputs/apk/release");
    let unsigned = ["app-release-unsigned.apk", "app-release.apk"].iter().map(|n| outs.join(n)).find(|p| p.is_file()).with_context(|| format!("gradle не оставил APK в {}", outs.display()))?;

    // zipalign + apksigner (подпись заменяет любую прежнюю)
    let work = app.work_dir("android");
    let aligned = work.join("aligned.apk");
    let apk = work.join(format!("{}-{}.apk", app.krate, app.version));
    let zipalign = build_tool("zipalign").context("нет zipalign (Android SDK build-tools)")?;
    let mut c = Command::new(zipalign);
    c.args(["-P", "16", "-f", "4"]).arg(&unsigned).arg(&aligned);
    util::run(c)?;
    let apksigner = build_tool("apksigner").context("нет apksigner (Android SDK build-tools)")?;
    util::step(format!("подпись: {} ({})", sign.keystore.display(), if sign.release { "релизный ключ" } else { "отладочный ключ" }));
    let mut c = Command::new(&apksigner);
    c.arg("sign").arg("--ks").arg(&sign.keystore).arg("--ks-key-alias").arg(&sign.alias);
    c.arg("--ks-pass").arg(format!("file:{}", sign.password_file.display()));
    c.arg("--out").arg(&apk).arg(&aligned);
    env(&mut c);
    util::run(c)?;
    let _ = std::fs::remove_file(&aligned);
    let _ = std::fs::remove_file(apk.with_extension("apk.idsig"));
    let size = std::fs::metadata(&apk).map(|m| m.len()).unwrap_or(0);
    util::note(format!("{} ({:.1} МБ)", apk.display(), size as f64 / 1048576.0));
    Ok(apk)
}

fn permission(p: &str) -> String {
    if p.contains('.') {
        p.to_string()
    } else {
        format!("android.permission.{p}")
    }
}

/// Сгенерировать gradle-проект из метаданных (перезаписывается при каждой сборке; файлы с тем же
/// содержимым не трогаются — gradle не пересобирает зря).
pub fn generate(app: &App, dir: &Path, lib: &str, abis: &[String]) -> Result<()> {
    let a = &app.meta.android;
    let id = a.id.clone().unwrap_or_else(|| app.app_id());
    let name = app.display_name();
    let min_sdk = a.min_sdk.unwrap_or(30);
    let target_sdk = a.target_sdk.unwrap_or(COMPILE_SDK);
    let w = |rel: &str, data: &str| util::write_if_changed(&dir.join(rel), data);

    w("settings.gradle.kts", &format!(
        "pluginManagement {{\n    repositories {{\n        google()\n        mavenCentral()\n        gradlePluginPortal()\n    }}\n}}\n\n\
         dependencyResolutionManagement {{\n    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)\n    repositories {{\n        google()\n        mavenCentral()\n    }}\n}}\n\n\
         rootProject.name = \"{}\"\ninclude(\":app\")\n",
        app.krate
    ))?;
    w("build.gradle.kts", &format!("// Сгенерировано syngui — правки делать в [package.metadata.syngui.android]\nplugins {{\n    id(\"com.android.application\") version \"{AGP}\" apply false\n}}\n"))?;
    w("gradle.properties", "android.useAndroidX=true\norg.gradle.jvmargs=-Xmx2048m\n")?;
    w("local.properties", &format!("sdk.dir={}\n", sdk_dir().display()))?;
    w("gradle/wrapper/gradle-wrapper.properties", WRAPPER_PROPS)?;
    util::write_if_changed(&dir.join("gradle/wrapper/gradle-wrapper.jar"), WRAPPER_JAR)?;
    util::write_if_changed(&dir.join("gradlew"), GRADLEW)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join("gradlew"), std::fs::Permissions::from_mode(0o755))?;
    }

    let abi_list = abis.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ");
    let deps = a.gradle_dependencies.iter().map(|d| format!("    implementation(\"{d}\")\n")).collect::<String>();
    let java_dirs = match &a.java {
        Some(j) => format!(", \"{}\"", app.path(j).display()),
        None => String::new(),
    };
    let res_dirs = match &a.res {
        Some(r) => format!(", \"{}\"", app.path(r).display()),
        None => String::new(),
    };
    w("app/build.gradle.kts", &format!(
        r#"// Сгенерировано syngui из [package.metadata.syngui] крейта {krate}.
plugins {{
    id("com.android.application")
}}

android {{
    namespace = "{id}"
    compileSdk = {COMPILE_SDK}

    defaultConfig {{
        applicationId = "{id}"
        minSdk = {min_sdk}
        targetSdk = {target_sdk}
        versionCode = {code}
        versionName = "{ver}"
        ndk {{
            abiFilters += listOf({abi_list})
        }}
    }}

    // Подпись — после сборки (syngui: zipalign + apksigner), ключ не попадает в проект.
    buildTypes {{
        getByName("release") {{
            isMinifyEnabled = false
        }}
    }}

    sourceSets {{
        getByName("main") {{
            jniLibs.srcDirs("src/main/jniLibs")
            java.srcDirs("src/main/java"{java_dirs})
            res.srcDirs("src/main/res"{res_dirs})
        }}
    }}

    packaging {{
        jniLibs {{
            useLegacyPackaging = false
        }}
    }}
}}

dependencies {{
    // Версия обязана совпадать с GameActivity, вшитой в android-activity, иначе RegisterNatives падает.
    implementation("androidx.games:games-activity:{GAMES_ACTIVITY}")
    // androidx.core (FileProvider для «Поделиться») и androidx.activity (системный фото-пикер).
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation(platform("org.jetbrains.kotlin:kotlin-bom:2.1.10"))
{deps}}}
"#,
        krate = app.krate,
        code = app.version_code(),
        ver = app.version,
    ))?;

    // Манифест
    let mut perms: Vec<String> = a.permissions.iter().map(|p| permission(p)).collect();
    let mut app_extra = String::new();
    if let Some(sub) = &a.foreground_service {
        for p in ["POST_NOTIFICATIONS", "FOREGROUND_SERVICE", "FOREGROUND_SERVICE_SPECIAL_USE"] {
            let p = permission(p);
            if !perms.contains(&p) {
                perms.push(p);
            }
        }
        app_extra.push_str(&format!(
            "        <!-- Foreground-сервис syngui (живой прогресс в уведомлении). -->\n        <service\n            android:name=\"syngui.android.SynGuiForegroundService\"\n            android:exported=\"false\"\n            android:foregroundServiceType=\"specialUse\">\n            <property\n                android:name=\"android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE\"\n                android:value=\"{}\" />\n        </service>\n",
            util::xml(sub)
        ));
        w("app/src/main/java/syngui/android/SynGuiForegroundService.java", FOREGROUND_SERVICE)?;
    } else {
        let _ = std::fs::remove_file(dir.join("app/src/main/java/syngui/android/SynGuiForegroundService.java"));
    }
    if let Some(x) = &a.manifest_application {
        app_extra.push_str(&read_snippet(app, x)?);
    }
    let manifest_extra = match &a.manifest {
        Some(x) => read_snippet(app, x)?,
        None => String::new(),
    };
    let perms_xml: String = perms.iter().map(|p| format!("    <uses-permission android:name=\"{}\" />\n", util::xml(p))).collect();
    let orientation = a.orientation.as_deref().map(|o| format!("\n            android:screenOrientation=\"{}\"", util::xml(o))).unwrap_or_default();
    w("app/src/main/AndroidManifest.xml", &format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!-- Сгенерировано syngui из [package.metadata.syngui] крейта {krate}. -->
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

{perms_xml}{manifest_extra}
    <application
        android:label="{label}"
        android:icon="@mipmap/ic_launcher"
        android:hasCode="true"
        android:allowBackup="true"
        android:isGame="false"
        android:enableOnBackInvokedCallback="true"
        android:theme="@style/Theme.SynGui">

        <!-- configChanges широкий: складные телефоны меняют размер и плотность экрана,
             пересоздание Activity перезапускало бы приложение. -->
        <activity
            android:name="{ACTIVITY}"
            android:exported="true"
            android:resizeableActivity="true"{orientation}
            android:configChanges="orientation|screenSize|smallestScreenSize|screenLayout|keyboardHidden|keyboard|navigation|uiMode|density|fontScale|layoutDirection|locale"
            android:windowSoftInputMode="adjustResize">

            <meta-data
                android:name="android.app.lib_name"
                android:value="{lib}" />

            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>

        <!-- syngui::platform::share_file / open_path: content:// для своих файлов.
             authority обязан быть «<пакет>.syngui.files». -->
        <provider
            android:name="androidx.core.content.FileProvider"
            android:authorities="${{applicationId}}.syngui.files"
            android:exported="false"
            android:grantUriPermissions="true">
            <meta-data
                android:name="android.support.FILE_PROVIDER_PATHS"
                android:resource="@xml/syngui_file_paths" />
        </provider>
{app_extra}    </application>
</manifest>
"#,
        krate = app.krate,
        label = util::xml(&name),
    ))?;

    let bg = a.background.as_deref().map(|c| format!("        <item name=\"android:windowBackground\">{}</item>\n", util::xml(c))).unwrap_or_default();
    let fs = if a.fullscreen { "        <item name=\"android:windowFullscreen\">true</item>\n" } else { "" };
    w("app/src/main/res/values/styles.xml", &format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <style name=\"Theme.SynGui\" parent=\"Theme.AppCompat.DayNight.NoActionBar\">\n        <item name=\"android:windowNoTitle\">true</item>\n{bg}{fs}        <item name=\"android:windowLayoutInDisplayCutoutMode\">shortEdges</item>\n    </style>\n</resources>\n"
    ))?;
    w("app/src/main/res/xml/syngui_file_paths.xml", "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- Корни, из которых syngui::platform выдаёт файлы другим приложениям. -->\n<paths>\n    <files-path name=\"files\" path=\".\" />\n    <cache-path name=\"cache\" path=\".\" />\n    <external-files-path name=\"external_files\" path=\".\" />\n    <external-cache-path name=\"external_cache\" path=\".\" />\n</paths>\n")?;

    // Значок
    let svg = match app.icon() {
        Some(p) => std::fs::read_to_string(&p).with_context(|| format!("значок {}", p.display()))?,
        None => {
            util::warn(format!("у {} нет значка (data/icons/<bin>.svg или [package.metadata.syngui] icon) — буква на круге", app.krate));
            letter_icon(&name)
        }
    };
    for (d, px) in [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)] {
        icon::render(&svg, px, None, &dir.join(format!("app/src/main/res/mipmap-{d}/ic_launcher.png")))?;
    }
    Ok(())
}

/// Фрагмент XML: строка с `<` — как есть, иначе путь к файлу.
fn read_snippet(app: &App, s: &str) -> Result<String> {
    let text = if s.trim_start().starts_with('<') { s.to_string() } else { std::fs::read_to_string(app.path(s)).with_context(|| format!("фрагмент манифеста {s}"))? };
    Ok(text.lines().map(|l| format!("        {l}\n")).collect())
}

/// Значок-заглушка: первая буква имени на круге.
pub fn letter_icon(name: &str) -> String {
    let ch = name.chars().next().unwrap_or('S').to_uppercase().to_string();
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><circle cx="64" cy="64" r="60" fill="#3d6df2"/><text x="64" y="86" font-family="sans-serif" font-size="64" font-weight="bold" fill="#fff" text-anchor="middle">{}</text></svg>"##,
        util::xml(&ch)
    )
}

/// Подключённое устройство (физическое — первым), либо $ANDROID_SERIAL.
pub fn device() -> Result<String> {
    if let Ok(s) = std::env::var("ANDROID_SERIAL") {
        return Ok(s);
    }
    let out = util::capture({
        let mut c = Command::new("adb");
        c.arg("devices");
        c
    })?;
    let devs: Vec<&str> = out.lines().skip(1).filter(|l| l.ends_with("\tdevice")).map(|l| l.split('\t').next().unwrap()).collect();
    devs.iter().find(|d| !d.starts_with("emulator-")).or(devs.first()).map(|s| s.to_string()).context("нет устройства Android (adb devices); включите отладку по USB")
}

/// Основной ABI устройства (ro.product.cpu.abi).
pub fn device_abi(dev: &str) -> Result<String> {
    let mut c = Command::new("adb");
    c.args(["-s", dev, "shell", "getprop", "ro.product.cpu.abi"]);
    let abi = util::capture(c)?.trim().to_string();
    if abi.is_empty() {
        bail!("не узнать ABI устройства {dev}");
    }
    Ok(abi)
}

/// Поставить и запустить; затем показывать logcat процесса.
pub fn install_run(app: &App, apk: &Path, logcat: bool) -> Result<()> {
    let id = app.meta.android.id.clone().unwrap_or_else(|| app.app_id());
    let dev = device()?;
    util::step(format!("adb install ({dev})"));
    let adb = |args: &[&str]| {
        let mut c = Command::new("adb");
        c.arg("-s").arg(&dev).args(args);
        c
    };
    let mut c = adb(&["install", "-r"]);
    c.arg(apk);
    if let Err(e) = util::run(c) {
        util::warn("установка не прошла: если подпись отличается от установленной версии — удалите её (adb uninstall) или подпишите тем же ключом");
        return Err(e);
    }
    util::step(format!("запуск {id}"));
    util::run(adb(&["shell", "am", "start", "-S", "-n", &format!("{id}/{ACTIVITY}")]))?;
    if !logcat {
        return Ok(());
    }
    let mut pid = String::new();
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let mut c = adb(&["shell", "pidof", &id]);
        c.stderr(std::process::Stdio::null());
        pid = c.output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        if !pid.is_empty() {
            break;
        }
    }
    if pid.is_empty() {
        util::warn("процесс не найден — похоже, упало на старте. Журнал падений:");
        return util::run(adb(&["logcat", "-b", "crash", "-d", "-t", "60"]));
    }
    util::step(format!("logcat pid {pid} (Ctrl+C — выход)"));
    util::run(adb(&["logcat", &format!("--pid={pid}")]))
}

/// Создать релизный ключ в ~/.secrets/<крейт>-android (каталог 700, файлы 600).
pub fn keygen(app: &App, dname: &str) -> Result<()> {
    let dir = key_dir(app);
    let ks = dir.join("release.jks");
    if ks.exists() {
        bail!("ключ уже есть: {} — не пересоздаю (потеря ключа = обновить установленное приложение нельзя)", ks.display());
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let mut pw = [0u8; 24];
    std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut pw))?;
    let pw: String = pw.iter().map(|b| (b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789")[*b as usize % 57] as char).collect();
    let pwf = dir.join("password");
    std::fs::write(&pwf, &pw)?;
    std::fs::set_permissions(&pwf, std::fs::Permissions::from_mode(0o600))?;
    let alias = app.meta.android.key_alias.clone().unwrap_or_else(|| app.krate.clone());
    let mut c = Command::new("keytool");
    c.args(["-genkeypair", "-keystore"]).arg(&ks).args(["-storetype", "PKCS12", "-alias", &alias, "-keyalg", "RSA", "-keysize", "4096", "-validity", "36500", "-dname", dname]);
    c.arg("-storepass:file").arg(&pwf).arg("-keypass:file").arg(&pwf);
    util::run(c)?;
    std::fs::set_permissions(&ks, std::fs::Permissions::from_mode(0o600))?;
    util::step(format!("ключ: {} (alias {alias}, RSA 4096, 100 лет)", ks.display()));
    let mut c = Command::new("keytool");
    c.args(["-list", "-v", "-keystore"]).arg(&ks).arg("-storepass:file").arg(&pwf);
    let out = util::capture(c)?;
    if let Some(l) = out.lines().find(|l| l.trim_start().starts_with("SHA256:")) {
        util::note(l.trim());
    }
    util::warn(format!("сохраните копию каталога {} — без него обновления приложения не поставятся", dir.display()));
    Ok(())
}

/// Включает ли набор возможностей крейта (с учётом вложенных) `syngui/ffmpeg-static`.
fn wants_ffmpeg_static(app: &App, features: &[String], default: bool) -> bool {
    let Ok(text) = std::fs::read_to_string(app.manifest_dir.join("Cargo.toml")) else { return false };
    let Ok(v) = text.parse::<toml::Value>() else { return false };
    let table = v.get("features").and_then(|f| f.as_table()).cloned().unwrap_or_default();
    let mut todo: Vec<String> = features.to_vec();
    if default {
        todo.push("default".into());
    }
    let mut seen = std::collections::HashSet::new();
    while let Some(f) = todo.pop() {
        if f.ends_with("ffmpeg-static") {
            return true;
        }
        if !seen.insert(f.clone()) {
            continue;
        }
        if let Some(list) = table.get(&f).and_then(|l| l.as_array()) {
            todo.extend(list.iter().filter_map(|x| x.as_str()).map(str::to_string));
        }
    }
    false
}
