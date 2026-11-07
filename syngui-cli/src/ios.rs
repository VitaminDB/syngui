//! iOS на Linux без Xcode: aarch64-apple-ios (clang + ld64.lld, iOS SDK из $IOS_SDK),
//! .app с Info.plist и значками → неподписанный .ipa. Подпись и установка — plumesign
//! (Plume Impactor) на Apple ID; он же подставляет свой профиль.

use crate::icon;
use crate::project::App;
use crate::target::{Profile, Target};
use crate::util;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn sdk() -> PathBuf {
    std::env::var_os("IOS_SDK").map(PathBuf::from).unwrap_or_else(|| util::home().join(".local/share/ios-sdk/src18/iPhoneOS18.6.sdk"))
}

/// Собрать .ipa; вернуть путь.
pub fn build(app: &App, profile: Profile) -> Result<PathBuf> {
    let m = &app.meta.ios;
    let sdk = sdk();
    if !sdk.join("SDKSettings.json").is_file() && !sdk.join("usr/include").is_dir() {
        bail!("нет iOS SDK: {} (IOS_SDK; не ниже 18 — иначе новые iPhone показывают приложение в режиме совместимости)", sdk.display());
    }
    let min_ios = m.min_ios.clone().unwrap_or_else(|| "14.0".into());
    let bin = app.bin()?;
    let s = sdk.display();

    let mut c = crate::linux::cargo(app, "build", profile);
    c.arg("--bin").arg(&bin).arg("--target").arg(Target::Ios.triple());
    if m.default_features == Some(false) {
        c.arg("--no-default-features");
    }
    if let Some(f) = &m.features {
        c.arg("--features").arg(f.join(","));
    }
    let cflags = format!("--target=arm64-apple-ios{min_ios} -isysroot {s}");
    c.env("SDKROOT", &sdk)
        .env("IPHONEOS_DEPLOYMENT_TARGET", &min_ios)
        .env("CC_aarch64_apple_ios", "clang")
        .env("CXX_aarch64_apple_ios", "clang++")
        .env("AR_aarch64_apple_ios", "llvm-ar")
        .env("CFLAGS_aarch64_apple_ios", &cflags)
        .env("CXXFLAGS_aarch64_apple_ios", &cflags)
        .env("CARGO_TARGET_AARCH64_APPLE_IOS_LINKER", "clang")
        .env(
            "CARGO_TARGET_AARCH64_APPLE_IOS_RUSTFLAGS",
            format!("-C link-arg=--target=arm64-apple-ios{min_ios} -C link-arg=-isysroot -C link-arg={s} -C link-arg=-fuse-ld=lld"),
        );
    util::step(format!("{bin}: ios, {}", profile.dir()));
    util::run(c)?;
    let built = app.target_dir.join(Target::Ios.triple()).join(profile.dir()).join(&bin);

    let out = app.work_dir("ios");
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    let dot_app = out.join("Payload").join(format!("{bin}.app"));
    util::copy(&built, &dot_app.join(&bin))?;
    {
        // полный strip (кроме --debug; у Mach-O — llvm-strip без ключей); strip = false — только локальные (-x)
        let full = m.strip.unwrap_or(profile != Profile::Dev);
        let mut c = Command::new("llvm-strip");
        if !full {
            c.arg("-x");
        }
        c.arg(dot_app.join(&bin));
        if let Err(e) = util::run(c) {
            util::warn(format!("llvm-strip: {e}"));
        }
    }

    // Значки: iPhone 60pt @2x/@3x, iPad 76pt @2x, 83.5pt @2x — квадрат во весь край на белом
    let svg = match m.icon.as_deref().map(|p| app.path(p)).or_else(|| app.icon()) {
        Some(p) => icon::load(&p)?,
        None => crate::android::letter_icon(&app.display_name()),
    };
    let svg = icon::square(&svg, m.icon_viewbox.as_deref(), m.icon_unround.as_deref());
    for (name, px) in [("AppIcon60x60@2x.png", 120), ("AppIcon60x60@3x.png", 180), ("AppIcon76x76@2x~ipad.png", 152), ("AppIcon83.5x83.5@2x~ipad.png", 167)] {
        icon::render(&svg, px, Some([255, 255, 255]), &dot_app.join(name))?;
    }

    let langs = app.meta.languages.clone().unwrap_or_else(|| vec!["ru".into(), "en".into()]);
    let sdk_ver = sdk.file_name().unwrap().to_string_lossy().trim_end_matches(".sdk").trim_start_matches("iPhoneOS").to_string();
    let name = util::xml(&app.display_name());
    let id = util::xml(&m.bundle_id.clone().unwrap_or_else(|| app.app_id()));
    let langs_xml: String = langs.iter().map(|l| format!("<string>{}</string>", util::xml(l))).collect();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>{dev}</string>
  <key>CFBundleLocalizations</key><array>{langs_xml}</array>
  <key>CFBundleExecutable</key><string>{bin}</string>
  <key>CFBundleIdentifier</key><string>{id}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundleDisplayName</key><string>{name}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{ver}</string>
  <key>CFBundleVersion</key><string>{build}</string>
  <key>CFBundleSupportedPlatforms</key><array><string>iPhoneOS</string></array>
  <key>DTPlatformName</key><string>iphoneos</string>
  <key>DTPlatformVersion</key><string>{sdk_ver}</string>
  <key>DTSDKName</key><string>iphoneos{sdk_ver}</string>
  <key>MinimumOSVersion</key><string>{min_ios}</string>
  <key>LSRequiresIPhoneOS</key><true/>
  <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
  <key>UIRequiredDeviceCapabilities</key><array><string>arm64</string><string>metal</string></array>
  <key>UILaunchScreen</key><dict/>
  <key>UIStatusBarHidden</key><{status}/>
  <key>UIRequiresFullScreen</key><true/>
  <key>UIApplicationSupportsIndirectInputEvents</key><true/>
  <key>UISupportedInterfaceOrientations</key>
  <array><string>UIInterfaceOrientationPortrait</string><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
  <key>UISupportedInterfaceOrientations~ipad</key>
  <array><string>UIInterfaceOrientationPortrait</string><string>UIInterfaceOrientationPortraitUpsideDown</string><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
  <key>ITSAppUsesNonExemptEncryption</key><false/>
  <key>CFBundleIcons</key>
  <dict><key>CFBundlePrimaryIcon</key><dict>
    <key>CFBundleIconFiles</key><array><string>AppIcon60x60</string></array>
  </dict></dict>
  <key>CFBundleIcons~ipad</key>
  <dict><key>CFBundlePrimaryIcon</key><dict>
    <key>CFBundleIconFiles</key><array><string>AppIcon60x60</string><string>AppIcon76x76</string><string>AppIcon83.5x83.5</string></array>
  </dict></dict>
</dict>
</plist>
"#,
        dev = util::xml(&langs[0]),
        ver = util::xml(&app.version),
        build = app.build_number(),
        status = if m.status_bar_hidden { "true" } else { "false" },
    );
    util::write(&dot_app.join("Info.plist"), plist)?;
    util::write(&dot_app.join("PkgInfo"), "APPL????")?;

    let ipa = out.join(format!("{}-{}.ipa", app.krate, app.version));
    let mut c = Command::new("zip");
    c.current_dir(&out).args(["-qr", "-X"]).arg(&ipa).arg("Payload");
    util::run(c)?;
    util::note(format!("{}", ipa.display()));
    Ok(ipa)
}

/// Подписать на Apple ID и поставить (plumesign); UDID — $SYNGUI_IOS_UDID или первое устройство.
pub fn install(ipa: &Path) -> Result<()> {
    if util::which("plumesign").is_none() {
        bail!("нет plumesign (Plume Impactor) — им .ipa подписывается на Apple ID и ставится на iPhone");
    }
    util::step("plumesign: подпись на Apple ID и установка");
    let mut c = Command::new("plumesign");
    c.args(["sign", "-p"]).arg(ipa).args(["--apple-id", "--register-and-install"]);
    if let Ok(u) = std::env::var("SYNGUI_IOS_UDID") {
        c.arg("--udid").arg(u);
    }
    util::run(c)?;
    util::note("падения: idevicecrashreport -k -e -f <программа> <каталог>; журнал: idevicesyslog");
    Ok(())
}
