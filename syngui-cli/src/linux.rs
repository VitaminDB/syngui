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

pub fn has_fast_release(root: &Path) -> bool {
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
pub fn cross_env(target_dir: &Path, c: &mut Command) -> Result<()> {
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
        let ff = target_dir.join("syngui/ffmpeg-cross-aarch64");
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

/// Собрать программу приложения; вернуть путь к бинарнику. `portable` — linux-x86_64 в контейнере
/// со старой glibc (запуск вне Arch: Ubuntu 20.04+, Debian 11+).
pub fn build(app: &App, target: Target, profile: Profile, extra: &[String], portable: bool) -> Result<PathBuf> {
    let bin = app.bin()?;
    if portable {
        if target != Target::LinuxX86_64 {
            bail!("--portable — только для linux-x86_64");
        }
        return portable_build(app, profile, extra);
    }
    let mut c = cargo(app, "build", profile);
    c.arg("--bin").arg(&bin);
    if target == Target::WindowsX86_64 {
        let w = &app.meta.windows;
        if w.default_features == Some(false) {
            c.arg("--no-default-features");
        }
        if let Some(f) = &w.features {
            c.arg("--features").arg(f.join(","));
        }
    } else if let Some(f) = &app.meta.linux.features {
        c.arg("--features").arg(f.join(","));
    }
    c.args(extra);
    let out = if target.is_host() {
        app.target_dir.join(profile.dir())
    } else {
        c.arg("--target").arg(target.triple());
        match target {
            Target::LinuxAarch64 => cross_env(&app.target_dir, &mut c)?,
            Target::WindowsX86_64 => {
                c.env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER", "x86_64-w64-mingw32-gcc")
                    .env("CC_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-gcc")
                    .env("CXX_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-g++")
                    .env("AR_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-ar");
            }
            _ => {}
        }
        app.target_dir.join(target.triple()).join(profile.dir())
    };
    util::step(format!("{bin}: {target}, {}", profile.dir()));
    util::run(c)?;
    let file = if target == Target::WindowsX86_64 { format!("{bin}.exe") } else { bin.clone() };
    let path = out.join(&file);
    if !path.is_file() {
        bail!("cargo не оставил {}", path.display());
    }
    util::note(format!("{}", path.display()));
    Ok(path)
}

/// Переносимая сборка: podman/docker, образ rust:1-bullseye (glibc 2.31). Рабочее пространство
/// монтируется по тому же пути, path-зависимости вне его (../syngui через [patch]) — только для чтения.
fn portable_build(app: &App, profile: Profile, extra: &[String]) -> Result<PathBuf> {
    let bin = app.bin()?;
    let run = util::which("podman").or_else(|| util::which("docker")).context("нет podman или docker — переносимая сборка идёт в контейнере")?;
    let image = app.meta.linux.portable_image.clone().unwrap_or_else(|| "docker.io/library/rust:1-bullseye".into());
    let apt = app.meta.linux.portable_packages.clone().unwrap_or_else(|| vec!["pkg-config".into()]);
    // каталоги path-зависимостей вне рабочего пространства
    let mut c = Command::new("cargo");
    c.current_dir(&app.manifest_dir).args(["metadata", "--format-version", "1"]);
    let meta: serde_json::Value = serde_json::from_str(&util::capture(c)?)?;
    let mut mounts: Vec<PathBuf> = Vec::new();
    for p in meta["packages"].as_array().into_iter().flatten() {
        if !p["source"].is_null() {
            continue;
        }
        let dir = PathBuf::from(p["manifest_path"].as_str().unwrap_or_default()).parent().unwrap().to_path_buf();
        if dir.starts_with(&app.workspace_root) {
            continue;
        }
        let mut g = Command::new("git");
        g.arg("-C").arg(&dir).args(["rev-parse", "--show-toplevel"]).stderr(std::process::Stdio::null());
        let top = g.output().ok().map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())).filter(|p| p.is_dir()).unwrap_or(dir);
        if !mounts.iter().any(|m| top.starts_with(m)) {
            mounts.push(top);
        }
    }
    let cache = util::home().join(".cache/syngui/portable");
    for d in ["registry", "git"] {
        std::fs::create_dir_all(cache.join(d))?;
    }
    let tdir = app.target_dir.join("portable");
    let cargo_args: Vec<String> = profile_args(app, profile).into_iter().chain(["--bin".into(), bin.clone()]).chain(app.meta.linux.features.iter().flat_map(|f| ["--features".to_string(), f.join(",")])).chain(extra.iter().cloned()).collect();
    let script = format!(
        "set -e; apt-get -qq update >/dev/null && apt-get -qq install -y {} >/dev/null; cargo build {} --target-dir {}",
        apt.iter().map(|a| util::sh(a)).collect::<Vec<_>>().join(" "),
        cargo_args.iter().map(|a| util::sh(a)).collect::<Vec<_>>().join(" "),
        util::sh(&tdir.display().to_string())
    );
    let mut c = Command::new(&run);
    c.args(["run", "--rm"]);
    let root = app.workspace_root.display().to_string();
    c.arg("-v").arg(format!("{root}:{root}"));
    for m in &mounts {
        c.arg("-v").arg(format!("{0}:{0}:ro", m.display()));
    }
    c.arg("-v").arg(format!("{}:/usr/local/cargo/registry", cache.join("registry").display()));
    c.arg("-v").arg(format!("{}:/usr/local/cargo/git", cache.join("git").display()));
    c.arg("-w").arg(&app.manifest_dir).arg(&image).args(["sh", "-c", &script]);
    util::step(format!("{bin}: linux-x86_64 переносимая ({image}), {}", profile.dir()));
    util::run(c)?;
    let path = tdir.join(profile.dir()).join(&bin);
    if !path.is_file() {
        bail!("контейнер не оставил {}", path.display());
    }
    if let Some(v) = max_glibc(&path) {
        util::note(format!("{} (нужна glibc ≥ {v})", path.display()));
    }
    Ok(path)
}

/// Наибольшая версия GLIBC_x.y среди символов программы.
pub fn max_glibc(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let mut best: Option<String> = None;
    let pat = b"GLIBC_";
    let mut i = 0;
    while let Some(p) = data[i..].windows(pat.len()).position(|w| w == pat) {
        let start = i + p + pat.len();
        let end = data[start..].iter().position(|b| !(b.is_ascii_digit() || *b == b'.')).map(|e| start + e).unwrap_or(data.len());
        let v = String::from_utf8_lossy(&data[start..end]).to_string();
        if !v.is_empty() && v.chars().next().unwrap().is_ascii_digit() && best.as_ref().is_none_or(|b| util::version_cmp(&v, b).is_gt()) {
            best = Some(v);
        }
        i = end.max(start);
    }
    best
}

/// Убрать символы: llvm-strip (ELF обеих архитектур) или mingw strip (exe).
pub fn strip(path: &Path, target: Target) {
    let tool = if target == Target::WindowsX86_64 { util::which("x86_64-w64-mingw32-strip") } else { util::which("llvm-strip").or_else(|| util::which("strip")) };
    let Some(tool) = tool else {
        util::warn(format!("нет программы strip для {target}"));
        return;
    };
    let mut c = Command::new(tool);
    if target != Target::WindowsX86_64 {
        c.arg("--strip-unneeded");
    }
    c.arg(path);
    if let Err(e) = util::run(c) {
        util::warn(format!("strip ({target}): {e}"));
    }
}

/// Имя значка из ярлыка (Icon=), иначе имя программы.
pub fn icon_name(app: &App) -> String {
    app.desktop_file()
        .and_then(|d| std::fs::read_to_string(d).ok())
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("Icon=").map(|s| s.trim().to_string())))
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .unwrap_or_else(|| app.bin().unwrap_or_else(|_| app.krate.clone()))
}

pub const PNG_SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];

/// Файлы приложения в дереве корня (usr/bin, ярлык, значки, документы, доп. файлы) — общая часть архива и пакета.
pub fn stage(app: &App, bin_path: &Path, root: &Path, strip_for: Option<Target>) -> Result<()> {
    let bin = app.bin()?;
    if root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    let dst = root.join("usr/bin").join(&bin);
    util::copy(bin_path, &dst)?;
    if let Some(t) = strip_for {
        strip(&dst, t);
    }
    let icon = icon_name(app);
    if let Some(d) = app.desktop_file() {
        // имя ярлыка сохраняется (kz.syn.synport.desktop — id AppStream)
        let name = if app.meta.desktop.is_some() { d.file_name().unwrap().to_string_lossy().into_owned() } else { format!("{bin}.desktop") };
        util::copy(&d, &root.join("usr/share/applications").join(name))?;
    }
    if let Some(i) = app.icon() {
        util::copy(&i, &root.join("usr/share/icons/hicolor/scalable/apps").join(format!("{icon}.svg")))?;
        // PNG по размерам: готовые по шаблону или нарисованные из SVG
        let svg = std::fs::read_to_string(&i).ok();
        for size in PNG_SIZES {
            let out = root.join(format!("usr/share/icons/hicolor/{size}x{size}/apps/{icon}.png"));
            match &app.meta.icons_png {
                Some(pat) => {
                    let src = app.path(&pat.replace("{size}", &size.to_string()));
                    if src.is_file() {
                        util::copy(&src, &out)?;
                    }
                }
                None => {
                    if let Some(svg) = &svg {
                        crate::icon::render(svg, size, None, &out)?;
                    }
                }
            }
        }
    }
    let pkg = app.meta.linux.package.clone().unwrap_or_else(|| bin.clone());
    for d in &app.meta.docs {
        let src = app.path(d);
        let name = src.file_name().unwrap().to_string_lossy().into_owned();
        let sub = if name.to_uppercase().starts_with("LICENSE") || name.to_uppercase().starts_with("COPYING") { "licenses" } else { "doc" };
        util::copy(&src, &root.join("usr/share").join(sub).join(&pkg).join(&name))?;
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
