//! Приложение из Cargo.toml: `cargo metadata` + раздел `[package.metadata.syngui]`.
//!
//! ```toml
//! [package.metadata.syngui]
//! name = "Мамины рецепты"          # имя под значком (по умолчанию — имя крейта)
//! id = "kz.syn.syncook"           # id приложения / bundle id
//! icon = "data/icons/syncook.svg" # SVG значка (по умолчанию data/icons/<bin>.svg)
//!
//! [package.metadata.syngui.android]
//! permissions = ["INTERNET"]
//!
//! [package.metadata.syngui.ios]
//! status_bar_hidden = true
//! ```
//! Полный список полей — в README крейта syngui-cli.

use crate::util;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Meta {
    pub name: Option<String>,
    pub id: Option<String>,
    pub icon: Option<String>,
    /// Программа (bin) приложения, если у крейта их несколько.
    pub bin: Option<String>,
    /// Ярлык .desktop (по умолчанию data/<bin>.desktop).
    pub desktop: Option<String>,
    /// Описание по-английски (AUR); по умолчанию package.description.
    pub description_en: Option<String>,
    /// Языки приложения (iOS CFBundleLocalizations); первый — основной.
    pub languages: Option<Vec<String>>,
    /// Документы в архивы и пакеты (README, лицензии): от каталога крейта.
    pub docs: Vec<String>,
    /// PNG-значки по размерам: шаблон с {size} ("assets/icon/synport-{size}.png"); иначе рисуются из SVG.
    pub icons_png: Option<String>,
    pub android: AndroidMeta,
    pub ios: IosMeta,
    pub linux: LinuxMeta,
    pub windows: WindowsMeta,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowsMeta {
    pub features: Option<Vec<String>>,
    pub default_features: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AndroidMeta {
    /// id приложения на Android, если отличается от общего `id`.
    pub id: Option<String>,
    /// Имя библиотеки (`android.app.lib_name`); по умолчанию `[lib] name`.
    pub lib: Option<String>,
    /// Возможности cargo для сборки под Android (по умолчанию ["android"]).
    pub features: Option<Vec<String>>,
    /// Включать возможности по умолчанию (по умолчанию нет).
    pub default_features: bool,
    /// ABI: arm64-v8a, armeabi-v7a, x86_64 (по умолчанию arm64-v8a).
    pub abis: Option<Vec<String>>,
    pub min_sdk: Option<u32>,
    pub target_sdk: Option<u32>,
    pub version_code: Option<u32>,
    /// Короткие имена (INTERNET) или полные (android.permission.INTERNET).
    pub permissions: Vec<String>,
    /// Статический FFmpeg из syngui/android-deps/ffmpeg/<abi> (FFMPEG_DIR).
    pub ffmpeg: bool,
    /// Убрать символы из .so (llvm-strip --strip-unneeded); по умолчанию — кроме --debug.
    pub strip: Option<bool>,
    /// Foreground-сервис syngui (живой прогресс в уведомлении): подтип specialUse.
    pub foreground_service: Option<String>,
    /// XML, вставляемый в <application> манифеста.
    pub manifest_application: Option<String>,
    /// XML, вставляемый в <manifest> (uses-feature и т. п.).
    pub manifest: Option<String>,
    /// Каталог ресурсов поверх сгенерированных (res/…).
    pub res: Option<String>,
    /// Каталог Java-исходников приложения.
    pub java: Option<String>,
    /// Цвет фона окна до первого кадра.
    pub background: Option<String>,
    /// Ориентация экрана (portrait, landscape…); по умолчанию свободная.
    pub orientation: Option<String>,
    /// Каталог ключа подписи (release.jks + password); по умолчанию ~/.secrets/<крейт>-android.
    pub keystore: Option<String>,
    pub key_alias: Option<String>,
    /// Своё gradle-приложение вместо сгенерированного (каталог с settings.gradle.kts).
    pub gradle_project: Option<String>,
    /// Доп. зависимости gradle (`implementation(...)`).
    pub gradle_dependencies: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IosMeta {
    pub bundle_id: Option<String>,
    pub min_ios: Option<String>,
    /// viewBox значка без полей (iOS сам скругляет углы): "8 8 112 112".
    pub icon_viewbox: Option<String>,
    /// Атрибут скругления фона в SVG, который убрать (`rx="30"` → `rx="0"`).
    pub icon_unround: Option<String>,
    /// Свой квадратный значок для iOS.
    pub icon: Option<String>,
    pub status_bar_hidden: bool,
    /// Убрать символы из программы; по умолчанию — кроме --debug (иначе только локальные, -x).
    pub strip: Option<bool>,
    /// Возможности cargo для iOS (по умолчанию — возможности крейта по умолчанию).
    pub features: Option<Vec<String>>,
    /// Включать возможности по умолчанию (по умолчанию да).
    pub default_features: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LinuxMeta {
    /// Имя пакета Arch/AUR (по умолчанию — имя программы).
    pub package: Option<String>,
    pub depends: Option<Vec<String>>,
    pub optdepends: Vec<String>,
    pub makedepends: Vec<String>,
    /// Возможности cargo для сборки под Linux.
    pub features: Option<Vec<String>>,
    /// Дополнительные файлы пакета: "откуда:куда" (куда — от корня, например usr/share/foo).
    pub files: Vec<String>,
    /// Свой установщик в архив tar (по умолчанию генерируется install.sh: ~/.local или /usr).
    pub installer: Option<String>,
    /// Образ для переносимой сборки (--portable): по умолчанию docker.io/library/rust:1-bullseye (glibc 2.31).
    pub portable_image: Option<String>,
    /// Пакеты apt в образе переносимой сборки (по умолчанию pkg-config).
    pub portable_packages: Option<Vec<String>>,
}

/// Приложение, выбранное для команды.
#[derive(Debug)]
pub struct App {
    pub krate: String,
    pub version: String,
    pub description: String,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub manifest_dir: PathBuf,
    pub workspace_root: PathBuf,
    pub target_dir: PathBuf,
    pub bins: Vec<String>,
    pub lib: Option<String>,
    pub meta: Meta,
    /// id из прежнего `[package.metadata.android] package` (cargo-apk), если нового нет.
    legacy_android_id: Option<String>,
}

#[derive(Deserialize)]
struct CargoMeta {
    packages: Vec<Pkg>,
    workspace_members: Vec<String>,
    workspace_default_members: Option<Vec<String>>,
    workspace_root: PathBuf,
    target_directory: PathBuf,
}

#[derive(Deserialize)]
struct Pkg {
    id: String,
    name: String,
    version: String,
    description: Option<String>,
    repository: Option<String>,
    license: Option<String>,
    manifest_path: PathBuf,
    targets: Vec<Target>,
    metadata: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}

impl App {
    /// Найти приложение: `-p крейт`, иначе крейт текущего каталога, иначе единственный с
    /// `[package.metadata.syngui]` в рабочем пространстве.
    pub fn load(manifest: Option<&Path>, package: Option<&str>) -> Result<App> {
        let mut c = Command::new("cargo");
        c.args(["metadata", "--format-version", "1", "--no-deps"]);
        if let Some(m) = manifest {
            c.arg("--manifest-path").arg(m);
        }
        let json = util::capture(c).context("cargo metadata")?;
        let m: CargoMeta = serde_json::from_str(&json).context("разбор cargo metadata")?;
        let cwd = std::env::current_dir()?;
        let has_meta = |p: &Pkg| p.metadata.as_ref().and_then(|v| v.get("syngui")).is_some();
        let pkg = if let Some(name) = package {
            m.packages.iter().find(|p| p.name == name).with_context(|| format!("нет крейта {name} в рабочем пространстве"))?
        } else if let Some(p) = m
            .packages
            .iter()
            .filter(|p| cwd.starts_with(p.manifest_path.parent().unwrap()))
            .max_by_key(|p| p.manifest_path.components().count())
        {
            p
        } else {
            let members: Vec<&Pkg> = m.packages.iter().filter(|p| m.workspace_members.contains(&p.id)).collect();
            let with: Vec<&&Pkg> = members.iter().filter(|p| has_meta(p)).collect();
            match with.as_slice() {
                [one] => one,
                _ => {
                    let def = m.workspace_default_members.as_deref().unwrap_or_default();
                    match def {
                        [one] => m.packages.iter().find(|p| &p.id == one).unwrap(),
                        _ => bail!(
                            "какое приложение? укажите -p <крейт> (с [package.metadata.syngui]: {})",
                            with.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                    }
                }
            }
        };
        let legacy = pkg.metadata.as_ref().and_then(|v| v.get("android"));
        let legacy_android_id = legacy.and_then(|a| a.get("package")).and_then(|v| v.as_str()).map(str::to_string);
        let mut meta: Meta = match pkg.metadata.as_ref().and_then(|v| v.get("syngui")) {
            Some(v) => serde_json::from_value(v.clone()).with_context(|| format!("[package.metadata.syngui] в {}", pkg.manifest_path.display()))?,
            None => Meta::default(),
        };
        if meta.android.min_sdk.is_none() {
            meta.android.min_sdk = legacy.and_then(|a| a.pointer("/sdk/min_sdk_version")).and_then(|v| v.as_u64()).map(|v| v as u32);
        }
        if meta.android.target_sdk.is_none() {
            meta.android.target_sdk = legacy.and_then(|a| a.pointer("/sdk/target_sdk_version")).and_then(|v| v.as_u64()).map(|v| v as u32);
        }
        let bins = pkg.targets.iter().filter(|t| t.kind.iter().any(|k| k == "bin")).map(|t| t.name.clone()).collect();
        let lib = pkg
            .targets
            .iter()
            .find(|t| t.kind.iter().any(|k| k == "cdylib" || k == "lib" || k == "rlib"))
            .map(|t| t.name.replace('-', "_"));
        Ok(App {
            krate: pkg.name.clone(),
            version: pkg.version.clone(),
            description: pkg.description.clone().unwrap_or_default(),
            repository: pkg.repository.clone(),
            license: pkg.license.clone(),
            manifest_dir: pkg.manifest_path.parent().unwrap().to_path_buf(),
            workspace_root: m.workspace_root,
            target_dir: m.target_directory,
            bins,
            lib,
            meta,
            legacy_android_id,
        })
    }

    /// Основная программа приложения.
    pub fn bin(&self) -> Result<String> {
        if let Some(b) = &self.meta.bin {
            return Ok(b.clone());
        }
        if self.bins.iter().any(|b| b == &self.krate) {
            return Ok(self.krate.clone());
        }
        match self.bins.as_slice() {
            [one] => Ok(one.clone()),
            [] => bail!("у крейта {} нет программы (bin)", self.krate),
            many => bail!("у крейта {} несколько программ ({}) — укажите [package.metadata.syngui] bin", self.krate, many.join(", ")),
        }
    }

    pub fn display_name(&self) -> String {
        self.meta.name.clone().unwrap_or_else(|| self.krate.clone())
    }

    /// id приложения: `id` из метаданных или io.github.vitamindb.<крейт без дефисов>.
    pub fn app_id(&self) -> String {
        self.meta.id.clone().or_else(|| self.legacy_android_id.clone()).unwrap_or_else(|| format!("io.github.vitamindb.{}", self.krate.replace(['-', '_'], "")))
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        let p = util::expand(rel);
        if p.is_absolute() {
            p
        } else {
            self.manifest_dir.join(p)
        }
    }

    /// SVG значка: из метаданных или data/icons/<bin>.svg.
    pub fn icon(&self) -> Option<PathBuf> {
        if let Some(i) = &self.meta.icon {
            return Some(self.path(i));
        }
        let bin = self.bin().unwrap_or_else(|_| self.krate.clone());
        [format!("data/icons/{bin}.svg"), format!("data/icons/{}.svg", self.krate), "data/icon.svg".into(), "assets/icon.svg".into()]
            .into_iter()
            .map(|r| self.manifest_dir.join(r))
            .find(|p| p.is_file())
    }

    pub fn desktop_file(&self) -> Option<PathBuf> {
        if let Some(d) = &self.meta.desktop {
            return Some(self.path(d));
        }
        let bin = self.bin().ok()?;
        let p = self.manifest_dir.join(format!("data/{bin}.desktop"));
        p.is_file().then_some(p)
    }

    /// Каталог служебных файлов syngui в target/.
    pub fn work_dir(&self, kind: &str) -> PathBuf {
        self.target_dir.join("syngui").join(kind).join(&self.krate)
    }

    /// Готовые артефакты (APK, IPA, архивы, пакеты).
    pub fn dist_dir(&self) -> PathBuf {
        self.target_dir.join("syngui").join("dist")
    }

    /// Номер сборки: число коммитов в git (как у iOS-скрипта), иначе 1.
    pub fn build_number(&self) -> u32 {
        let mut c = Command::new("git");
        c.arg("-C").arg(&self.manifest_dir).args(["rev-list", "--count", "HEAD"]);
        c.stderr(std::process::Stdio::null());
        c.output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(1)
    }

    /// versionCode Android из семантической версии: 1.2.3 → 1002003 (растёт с каждой версией).
    pub fn version_code(&self) -> u32 {
        if let Some(c) = self.meta.android.version_code {
            return c;
        }
        let v: Vec<u32> = self.version.split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect();
        let g = |i: usize| v.get(i).copied().unwrap_or(0);
        (g(0) * 1_000_000 + g(1) * 1_000 + g(2)).max(1)
    }
}

/// Каталог исходников syngui (android-deps, Java-классы): $SYNGUI_DIR или репозиторий, из которого собрана утилита.
pub fn syngui_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SYNGUI_DIR") {
        return PathBuf::from(d);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
