//! `syngui doctor` — что есть для сборки под каждую цель (как `flutter doctor`).

use crate::target::Target;
use crate::{android, ios, linux, util};
use std::path::Path;
use std::process::Command;

struct Report {
    ok: bool,
}

impl Report {
    fn check(&mut self, ok: bool, what: &str, detail: impl AsRef<str>, hint: &str) {
        if ok {
            println!("  \x1b[32m✓\x1b[0m {what}: {}", detail.as_ref());
        } else {
            self.ok = false;
            println!("  \x1b[31m✗\x1b[0m {what}: {}", detail.as_ref());
            if !hint.is_empty() {
                println!("      → {hint}");
            }
        }
    }
    fn tool(&mut self, name: &str, hint: &str) {
        match util::which(name) {
            Some(p) => self.check(true, name, p.display().to_string(), ""),
            None => self.check(false, name, "нет", hint),
        }
    }
    fn path(&mut self, what: &str, p: Option<&Path>, hint: &str) {
        match p {
            Some(p) if p.exists() => self.check(true, what, p.display().to_string(), ""),
            Some(p) => self.check(false, what, format!("нет {}", p.display()), hint),
            None => self.check(false, what, "не найден", hint),
        }
    }
}

fn rust_targets() -> Vec<String> {
    Command::new("rustup").args(["target", "list", "--installed"]).output().map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect()).unwrap_or_default()
}

/// Проверить цели; вернуть, всё ли готово.
pub fn run(targets: &[Target]) -> bool {
    let installed = rust_targets();
    let mut all = true;
    let has_target = |r: &mut Report, t: &str| r.check(installed.iter().any(|i| i == t), &format!("rust {t}"), if installed.iter().any(|i| i == t) { "есть" } else { "нет" }, &format!("rustup target add {t}"));
    {
        println!("\x1b[1mОбщее\x1b[0m");
        let mut r = Report { ok: true };
        r.tool("cargo", "https://rustup.rs");
        r.tool("rustup", "pacman -S rustup");
        r.tool("clang", "pacman -S clang");
        r.tool("mold", "pacman -S mold (быстрая линковка, ~/.cargo/config.toml)");
        all &= r.ok;
    }
    for t in targets {
        println!("\x1b[1m{t}\x1b[0m");
        let mut r = Report { ok: true };
        match t {
            Target::LinuxX86_64 | Target::LinuxAarch64 => {
                has_target(&mut r, t.triple());
                if !t.is_host() && *t == Target::LinuxAarch64 {
                    r.tool("ld.lld", "pacman -S lld");
                    r.tool("llvm-ar", "pacman -S llvm");
                    r.path("sysroot aarch64", linux::aarch64_sysroot().as_deref(), "SYNGUI_AARCH64_SYSROOT=<rootfs с glibc> или synport rootfs");
                }
                r.tool("tar", "");
                r.tool("zstd", "pacman -S zstd");
                r.tool("makepkg", "пакеты Arch: pacman -S pacman");
            }
            Target::Android => {
                has_target(&mut r, "aarch64-linux-android");
                r.tool("cargo-ndk", "cargo install cargo-ndk");
                let sdk = android::sdk_dir();
                r.path("Android SDK", Some(&sdk), "ANDROID_HOME=<sdk> (sdkmanager \"platforms;android-35\" \"build-tools;35.0.0\")");
                let plat = util::latest_subdir(&sdk.join("platforms"));
                r.path("платформа SDK", plat.as_deref(), "sdkmanager \"platforms;android-35\"");
                r.path("NDK", android::ndk_dir().as_deref(), "ANDROID_NDK_HOME=<ndk> или sdkmanager \"ndk;27.2.12479018\"");
                r.path("JDK 17–21", android::java_home().as_deref(), "pacman -S jdk21-openjdk (или JAVA_HOME)");
                r.path("zipalign", android::build_tool("zipalign").as_deref(), "sdkmanager \"build-tools;35.0.0\"");
                r.path("apksigner", android::build_tool("apksigner").as_deref(), "sdkmanager \"build-tools;35.0.0\"");
                r.tool("keytool", "входит в JDK");
                r.tool("adb", "pacman -S android-tools (установка на телефон)");
                let ff = crate::project::syngui_dir().join("android-deps/ffmpeg/arm64-v8a/lib/libavcodec.a");
                let ok = ff.is_file();
                r.check(ok || true, "статический FFmpeg (android.ffmpeg)", if ok { ff.display().to_string() } else { "нет (нужен только с android.ffmpeg = true: syngui/scripts/build-ffmpeg-android.sh)".into() }, "");
            }
            Target::Ios => {
                has_target(&mut r, "aarch64-apple-ios");
                let sdk = ios::sdk();
                r.path("iOS SDK", Some(&sdk), "IOS_SDK=<iPhoneOS18.x.sdk> (github.com/xybp888/iOS-SDKs; не ниже 18)");
                r.tool("ld.lld", "pacman -S lld");
                r.tool("llvm-ar", "pacman -S llvm");
                r.tool("zip", "pacman -S zip");
                r.tool("plumesign", "Plume Impactor — подпись на Apple ID и установка");
                r.tool("idevicesyslog", "pacman -S libimobiledevice (журнал iPhone)");
            }
        }
        all &= r.ok;
    }
    println!();
    if all {
        println!("\x1b[32mВсё готово.\x1b[0m");
    } else {
        println!("\x1b[33mЕсть недостающее (✗) — см. подсказки.\x1b[0m");
    }
    all
}
