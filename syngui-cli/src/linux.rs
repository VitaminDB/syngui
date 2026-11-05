//! Linux: x86_64 и aarch64 (телефоны). Кросс-сборка aarch64 — clang + lld с glibc из sysroot
//! (rootfs телефона из synmobile), как `synport cross`.

use crate::project::App;
use crate::target::{Profile, Target};
use crate::util;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Команда `cargo <sub>` для приложения с нужным профилем (fast-release, если в Cargo.toml его нет, задаётся через --config).
pub fn cargo(app: &App, sub: &str, profile: Profile) -> Command {
    let mut c = Command::new("cargo");
    c.current_dir(&app.manifest_dir).arg(sub).args(profile_args(app, profile));
    c
}

/// Аргументы профиля и пакета: `--profile fast-release -p <крейт>`.
pub fn profile_args(app: &App, profile: Profile) -> Vec<String> {
    let mut v = Vec::new();
    if profile == Profile::Fast && !has_fast_release(&app.workspace_root) {
        for kv in ["inherits=\"release\"", "lto=false", "codegen-units=256", "incremental=true", "debug=false"] {
            v.push("--config".to_string());
            v.push(format!("profile.fast-release.{kv}"));
        }
    }
    v.extend(profile.cargo_args());
    v.push("-p".into());
    v.push(app.krate.clone());
    v
}

fn has_fast_release(root: &Path) -> bool {
    std::fs::read_to_string(root.join("Cargo.toml")).map(|s| s.contains("[profile.fast-release]")).unwrap_or(false)
}

/// sysroot aarch64: $SYNGUI_AARCH64_SYSROOT, ~/.local/share/syngui/sysroot-aarch64, rootfs телефона из synmobile.
pub fn aarch64_sysroot() -> Option<PathBuf> {
    let ok = |p: &Path| p.join("usr/lib/libc.so.6").is_file() || p.join("usr/lib/libc.so").is_file();
    if let Some(p) = std::env::var_os("SYNGUI_AARCH64_SYSROOT") {
        return Some(PathBuf::from(p));
    }
    let home = util::home();
    let mut cands = vec![home.join(".local/share/syngui/sysroot-aarch64")];
    for base in [home.join("Storage/Проекты/2027/synmobile/devices"), home.join("Projects/2027/synmobile/devices")] {
        if let Ok(rd) = std::fs::read_dir(&base) {
            let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path().join("build/rootfs")).collect();
            v.sort();
            // xiaomi-diting — основной телефон; его rootfs первым
            v.sort_by_key(|p| !p.to_string_lossy().contains("xiaomi-diting"));
            cands.extend(v);
        }
    }
    cands.into_iter().find(|p| ok(p))
}

/// Окружение кросс-сборки под aarch64-unknown-linux-gnu (и build.rs внутри).
fn cross_env(app: &App, c: &mut Command) -> Result<()> {
    let Some(sysroot) = aarch64_sysroot() else {
        bail!("нет sysroot для aarch64 (glibc и библиотеки телефона): задайте SYNGUI_AARCH64_SYSROOT или соберите rootfs (synport rootfs)");
    };
    let sr = sysroot.display();
    let t = "aarch64-unknown-linux-gnu";
    c.env("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER", "clang")
        .env(
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            format!("-C link-arg=--target={t} -C link-arg=-fuse-ld=lld -C link-arg=--sysroot={sr} -C link-arg=-Wl,--dynamic-linker=/lib/ld-linux-aarch64.so.1 -C link-arg=-Wl,-rpath-link,{sr}/usr/lib"),
        )
        .env("CC_aarch64_unknown_linux_gnu", format!("clang --target={t} --sysroot={sr}"))
        .env("CXX_aarch64_unknown_linux_gnu", format!("clang++ --target={t} --sysroot={sr}"))
        .env("AR_aarch64_unknown_linux_gnu", "llvm-ar")
        .env("BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu", format!("--target={t} --sysroot={sr}"))
        .env("PKG_CONFIG_SYSROOT_DIR", &sysroot)
        .env("PKG_CONFIG_LIBDIR", format!("{sr}/usr/lib/pkgconfig:{sr}/usr/share/pkgconfig"))
        .env("PKG_CONFIG_ALLOW_CROSS", "1");
    // ffmpeg-sys-next проверяет версию программой под хост: заголовки libav* — отдельным каталогом
    // (без glibc sysroot), библиотеки — из sysroot.
    if sysroot.join("usr/include/libavcodec").is_dir() {
        let ff = app.target_dir.join("syngui/ffmpeg-cross-aarch64");
        std::fs::create_dir_all(ff.join("include"))?;
        for e in std::fs::read_dir(sysroot.join("usr/include"))?.filter_map(|e| e.ok()) {
            let n = e.file_name().to_string_lossy().into_owned();
            if (n.starts_with("libav") || n.starts_with("libsw") || n.starts_with("libpostproc")) && e.path().is_dir() {
                let l = ff.join("include").join(&n);
                let _ = std::fs::remove_file(&l);
                std::os::unix::fs::symlink(e.path(), &l)?;
            }
        }
        let l = ff.join("lib");
        let _ = std::fs::remove_file(&l);
        std::os::unix::fs::symlink(sysroot.join("usr/lib"), &l)?;
        c.env("FFMPEG_DIR", &ff);
    }
    Ok(())
}

/// Собрать программу приложения; вернуть путь к бинарнику.
pub fn build(app: &App, target: Target, profile: Profile, extra: &[String]) -> Result<PathBuf> {
    let bin = app.bin()?;
    let mut c = cargo(app, "build", profile);
    c.arg("--bin").arg(&bin);
    if let Some(f) = &app.meta.linux.features {
        c.arg("--features").arg(f.join(","));
    }
    c.args(extra);
    let out = if target.is_host() {
        app.target_dir.join(profile.dir())
    } else {
        c.arg("--target").arg(target.triple());
        if target == Target::LinuxAarch64 {
            cross_env(app, &mut c)?;
        }
        app.target_dir.join(target.triple()).join(profile.dir())
    };
    util::step(format!("{bin}: {target}, {}", profile.dir()));
    util::run(c)?;
    let path = out.join(&bin);
    if !path.is_file() {
        bail!("cargo не оставил {}", path.display());
    }
    util::note(format!("{}", path.display()));
    Ok(path)
}

/// Файлы приложения в дереве корня (usr/bin, ярлык, значок, доп. файлы) — общая часть архива и пакета.
pub fn stage(app: &App, bin_path: &Path, root: &Path, strip_for: Option<Target>) -> Result<()> {
    let bin = app.bin()?;
    if root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    let dst = root.join("usr/bin").join(&bin);
    util::copy(bin_path, &dst)?;
    if let Some(t) = strip_for {
        // llvm-strip понимает обе архитектуры
        let mut c = Command::new(util::which("llvm-strip").map(|p| p.into_os_string()).unwrap_or_else(|| "strip".into()));
        c.arg("--strip-unneeded").arg(&dst);
        if let Err(e) = util::run(c) {
            util::warn(format!("strip ({t}): {e}"));
        }
    }
    if let Some(d) = app.desktop_file() {
        util::copy(&d, &root.join("usr/share/applications").join(format!("{bin}.desktop")))?;
    }
    if let Some(i) = app.icon() {
        util::copy(&i, &root.join("usr/share/icons/hicolor/scalable/apps").join(format!("{bin}.svg")))?;
    }
    for f in &app.meta.linux.files {
        let (from, to) = f.split_once(':').with_context(|| format!("linux.files: «{f}» — нужно «откуда:куда»"))?;
        let from = app.path(from);
        let to = root.join(to.trim_start_matches('/'));
        if from.is_dir() {
            util::copy_tree(&from, &to)?;
        } else {
            util::copy(&from, &to)?;
        }
    }
    Ok(())
}
