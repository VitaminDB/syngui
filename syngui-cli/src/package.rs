//! Упаковка: tar.zst с готовыми файлами (основа -bin), пакет pacman, PKGBUILD'ы AUR -git и -bin.

use crate::linux;
use crate::project::App;
use crate::target::{Profile, Target};
use crate::util::{self, sh};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Имя пакета Arch/AUR.
pub fn pkgname(app: &App) -> Result<String> {
    Ok(app.meta.linux.package.clone().unwrap_or(app.bin()?))
}

/// Имя архива релиза: `<программа>-<версия>-<арх>.tar.zst` (его скачивает -bin).
pub fn tar_name(app: &App, target: Target) -> Result<String> {
    Ok(format!("{}-{}-{}.tar.zst", app.bin()?, app.version, target.arch()))
}

/// Собрать и упаковать в tar.zst (+ .sha256) в dist: дерево usr/… (его ставит -bin и пакет Arch),
/// рядом — документы и install.sh (установка вне Arch: ~/.local или /usr).
pub fn tar(app: &App, target: Target, profile: Profile, portable: bool, extra: &[String]) -> Result<PathBuf> {
    let bin = linux::build(app, target, profile, extra, portable)?;
    let root = app.work_dir("stage").join(target.arch());
    linux::stage(app, &bin, &root, Some(target))?;
    let mut top = vec!["usr".to_string()];
    for d in &app.meta.docs {
        let src = app.path(d);
        let name = src.file_name().unwrap().to_string_lossy().into_owned();
        util::copy(&src, &root.join(&name))?;
        top.push(name);
    }
    let inst = root.join("install.sh");
    match &app.meta.linux.installer {
        Some(i) => util::copy(&app.path(i), &inst)?,
        None => util::write(&inst, installer(app)?)?,
    }
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&inst, std::fs::Permissions::from_mode(0o755))?;
    }
    top.push("install.sh".into());
    let dist = app.dist_dir();
    std::fs::create_dir_all(&dist)?;
    let out = dist.join(tar_name(app, target)?);
    util::step(format!("архив {}", out.display()));
    let mut c = Command::new("tar");
    c.arg("--zstd").arg("--owner=0").arg("--group=0").arg("--numeric-owner").arg("-cf").arg(&out).arg("-C").arg(&root).args(&top);
    util::run(c)?;
    util::sha256_file(&out)?;
    Ok(out)
}

/// install.sh архива: дерево usr/ → $1 (по умолчанию ~/.local), обновить кэши значков и ярлыков.
fn installer(app: &App) -> Result<String> {
    let bin = app.bin()?;
    Ok(format!(
        r#"#!/bin/sh
# Установить {bin}: ./install.sh — в ~/.local (без root); sudo ./install.sh /usr — в систему.
set -eu
prefix=${{1:-$HOME/.local}}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$prefix"
cp -r "$here/usr/." "$prefix/"
chmod 755 "$prefix/bin/{bin}"
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" 2>/dev/null || true
command -v update-desktop-database >/dev/null && update-desktop-database -q "$prefix/share/applications" 2>/dev/null || true
echo "{bin}: $prefix/bin/{bin}"
"#
    ))
}

/// Windows: `<программа>-<версия>-windows-x86_64.zip` — exe и документы.
pub fn zip(app: &App, profile: Profile) -> Result<PathBuf> {
    let exe = linux::build(app, Target::WindowsX86_64, profile, &[], false)?;
    let dir = app.work_dir("stage").join("windows");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let staged = dir.join(exe.file_name().unwrap());
    util::copy(&exe, &staged)?;
    linux::strip(&staged, Target::WindowsX86_64);
    let mut files = vec![staged];
    for d in &app.meta.docs {
        let src = app.path(d);
        let dst = dir.join(src.file_name().unwrap());
        util::copy(&src, &dst)?;
        files.push(dst);
    }
    let dist = app.dist_dir();
    std::fs::create_dir_all(&dist)?;
    let out = dist.join(format!("{}-{}-windows-x86_64.zip", app.bin()?, app.version));
    let _ = std::fs::remove_file(&out);
    util::step(format!("архив {}", out.display()));
    let mut c = Command::new("zip");
    c.arg("-qj9X").arg(&out).args(&files);
    util::run(c)?;
    util::sha256_file(&out)?;
    Ok(out)
}

/// SHA256SUMS по всем артефактам dist (кроме .sha256).
pub fn sha256sums(dist: &Path) -> Result<PathBuf> {
    let mut names: Vec<String> = std::fs::read_dir(dist)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.ends_with(".sha256") && n != "SHA256SUMS")
        .collect();
    names.sort();
    let out = dist.join("SHA256SUMS");
    if names.is_empty() {
        return Ok(out);
    }
    let mut c = Command::new("sha256sum");
    c.current_dir(dist).args(&names);
    util::write(&out, util::capture(c)?)?;
    Ok(out)
}

fn arr(v: &[String]) -> String {
    v.iter().map(|s| sh(s)).collect::<Vec<_>>().join(" ")
}

fn depends(app: &App) -> Vec<String> {
    app.meta.linux.depends.clone().unwrap_or_else(|| vec!["gcc-libs".into(), "glibc".into(), "fontconfig".into(), "libxkbcommon".into(), "wayland".into(), "vulkan-icd-loader".into()])
}

fn license(app: &App) -> String {
    app.license.clone().unwrap_or_else(|| "GPL-3.0-only".into())
}

fn desc_en(app: &App) -> String {
    app.meta.description_en.clone().unwrap_or_else(|| app.description.clone())
}

/// Строки установки файлов из дерева stage (общие для -bin и локального пакета): `cp -a usr "$pkgdir"/`.
const INSTALL_TREE: &str = "    cp -a --no-preserve=ownership usr \"$pkgdir\"/\n";

/// Пакет pacman из собранных файлов: PKGBUILD в target/syngui/arch/<крейт>, makepkg.
pub fn arch(app: &App, target: Target, profile: Profile, portable: bool, extra: &[String]) -> Result<PathBuf> {
    let tar = tar(app, target, profile, portable, extra)?;
    let name = pkgname(app)?;
    let dir = app.work_dir("arch").join(target.arch());
    std::fs::create_dir_all(&dir)?;
    let tarname = tar.file_name().unwrap().to_string_lossy().into_owned();
    util::copy(&tar, &dir.join(&tarname))?;
    let pkgver = app.version.replace('-', "_");
    let pkgbuild = format!(
        "# Сгенерировано syngui package --format arch\npkgname={name}\npkgver={pkgver}\npkgrel={rel}\npkgdesc={desc}\narch=({arch})\nurl={url}\nlicense=({lic})\ndepends=({deps})\n{opt}options=('!strip' '!debug')\nsource=({src})\nsha256sums=('SKIP')\n\npackage() {{\n{INSTALL_TREE}}}\n",
        rel = app.build_number(),
        desc = sh(&app.description),
        arch = sh(target.arch()),
        url = sh(app.repository.as_deref().unwrap_or("")),
        lic = sh(&license(app)),
        deps = arr(&depends(app)),
        opt = if app.meta.linux.optdepends.is_empty() { String::new() } else { format!("optdepends=({})\n", arr(&app.meta.linux.optdepends)) },
        src = sh(&tarname),
    );
    util::write(&dir.join("PKGBUILD"), pkgbuild)?;
    util::step(format!("makepkg ({})", target.arch()));
    let mut c = Command::new("makepkg");
    // Архитектура пакета — цели, а не машины: свой makepkg.conf поверх системного
    let conf = dir.join("makepkg.conf");
    util::write(&conf, format!("source /etc/makepkg.conf\nCARCH={}\nCHOST={}\nPKGDEST={}\n", target.arch(), target.triple(), sh(&app.dist_dir().display().to_string())))?;
    c.current_dir(&dir).arg("--config").arg(&conf).args(["-f", "--noconfirm", "--nodeps", "--noextract"]).env("PKGDEST", app.dist_dir());
    // --noextract: распакованный архив кладём сами (makepkg не распаковывает .tar.zst без bsdtar-магии в srcdir)
    let src = dir.join("src");
    if src.exists() {
        std::fs::remove_dir_all(&src)?;
    }
    std::fs::create_dir_all(&src)?;
    let mut x = Command::new("tar");
    x.arg("-xf").arg(&tar).arg("-C").arg(&src);
    util::run(x)?;
    util::run(c)?;
    let pkg = std::fs::read_dir(app.dist_dir())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with(&format!("{name}-{pkgver}-")) && n.ends_with(&format!("-{}.pkg.tar.zst", target.arch()))
        })
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let pkg = pkg.context("makepkg не оставил пакет")?;
    util::note(format!("{}", pkg.display()));
    Ok(pkg)
}

/// URL репозитория из `git remote origin` (git@github.com:a/b.git → https://github.com/a/b).
fn git_origin(app: &App) -> Option<String> {
    let mut c = Command::new("git");
    c.arg("-C").arg(&app.manifest_dir).args(["remote", "get-url", "origin"]).stderr(std::process::Stdio::null());
    let url = String::from_utf8(c.output().ok()?.stdout).ok()?.trim().to_string();
    if url.is_empty() {
        return None;
    }
    let url = match url.strip_prefix("git@") {
        Some(rest) => format!("https://{}", rest.replacen(':', "/", 1)),
        None => url,
    };
    Some(url.trim_end_matches(".git").to_string())
}

/// Корень репозитория git и путь крейта в нём (для -git).
fn repo_rel(app: &App) -> Result<(PathBuf, String)> {
    let mut c = Command::new("git");
    c.arg("-C").arg(&app.manifest_dir).args(["rev-parse", "--show-toplevel"]);
    let top = PathBuf::from(util::capture(c)?.trim());
    let rel = app.manifest_dir.strip_prefix(&top).unwrap_or(Path::new("")).to_string_lossy().into_owned();
    Ok((top, rel))
}

/// PKGBUILD'ы AUR `<имя>-git` (сборка из репозитория) и `<имя>-bin` (архивы релиза GitHub,
/// x86_64 и aarch64; суммы — из dist/*.sha256, если архивы уже собраны) + .SRCINFO.
pub fn aur(app: &App, out: Option<&Path>) -> Result<PathBuf> {
    let repo = app.repository.clone().or_else(|| git_origin(app)).context("нет package.repository в Cargo.toml и git remote origin — URL нужен для AUR")?;
    let repo = repo.trim_end_matches('/').trim_end_matches(".git").to_string();
    let name = pkgname(app)?;
    let bin = app.bin()?;
    let out = out.map(Path::to_path_buf).unwrap_or_else(|| app.target_dir.join("syngui/aur"));
    let (_top, rel) = repo_rel(app)?;
    let repo_name = repo.rsplit('/').next().unwrap_or("src").to_string();
    let cd = if rel.is_empty() { format!("\"$srcdir/{repo_name}\"") } else { format!("\"$srcdir/{repo_name}/{rel}\"") };
    let maint = "# Maintainer: Alexeyev Vitaly\n";
    let deps = arr(&depends(app));
    let opt = if app.meta.linux.optdepends.is_empty() { String::new() } else { format!("optdepends=({})\n", arr(&app.meta.linux.optdepends)) };
    let mut mk = vec!["cargo".to_string(), "git".into(), "clang".into(), "pkgconf".into()];
    mk.extend(app.meta.linux.makedepends.iter().cloned());
    // возможности по архитектуре (linux.x86_64 / linux.aarch64 поверх общих)
    let fx = arr(&app.linux_feature_args("x86_64"));
    let fa = arr(&app.linux_feature_args("aarch64"));
    let features_sel = format!("    local _f=({fx})\n    [ \"$CARCH\" = aarch64 ] && _f=({fa})\n");

    // Установка файлов -git: та же раскладка, что stage()
    let rel = |p: &Path| p.strip_prefix(&app.manifest_dir).unwrap_or(p).display().to_string();
    let icon = linux::icon_name(app);
    let mut inst = format!("    install -Dm755 \"$CARGO_TARGET_DIR/release/{bin}\" \"$pkgdir/usr/bin/{bin}\"\n");
    if let Some(d) = app.desktop_file() {
        let name = if app.meta.desktop.is_some() { d.file_name().unwrap().to_string_lossy().into_owned() } else { format!("{bin}.desktop") };
        inst.push_str(&format!("    install -Dm644 {} \"$pkgdir/usr/share/applications/{name}\"\n", sh(&rel(&d))));
    }
    if let Some(i) = app.icon() {
        if crate::icon::is_raster(&i) {
            let (w, h) = crate::icon::png_size(&i)?;
            inst.push_str(&format!("    install -Dm644 {} \"$pkgdir/usr/share/icons/hicolor/{w}x{h}/apps/{icon}.png\"\n", sh(&rel(&i))));
        } else {
            inst.push_str(&format!("    install -Dm644 {} \"$pkgdir/usr/share/icons/hicolor/scalable/apps/{icon}.svg\"\n", sh(&rel(&i))));
        }
    }
    if let Some(pat) = &app.meta.icons_png {
        for size in linux::PNG_SIZES {
            let p = app.path(&pat.replace("{size}", &size.to_string()));
            if p.is_file() {
                inst.push_str(&format!("    install -Dm644 {} \"$pkgdir/usr/share/icons/hicolor/{size}x{size}/apps/{icon}.png\"\n", sh(&rel(&p))));
            }
        }
    }
    let pkgn = name.clone();
    for d in &app.meta.docs {
        let p = app.path(d);
        let n = p.file_name().unwrap().to_string_lossy().into_owned();
        let sub = if n.to_uppercase().starts_with("LICENSE") || n.to_uppercase().starts_with("COPYING") { "licenses" } else { "doc" };
        inst.push_str(&format!("    install -Dm644 {} \"$pkgdir/usr/share/{sub}/{pkgn}/{n}\"\n", sh(&rel(&p))));
    }
    for f in &app.meta.linux.files {
        let (from, to) = f.split_once(':').with_context(|| format!("linux.files: «{f}»"))?;
        let to = to.trim_start_matches('/');
        let parent = Path::new(to).parent().map(|p| p.display().to_string()).unwrap_or_default();
        inst.push_str(&format!("    install -dm755 \"$pkgdir/{parent}\"\n    cp -r {} \"$pkgdir/{to}\"\n", sh(from)));
    }

    let git = format!(
        "{maint}pkgname={name}-git\npkgver={ver}.r0\npkgrel=1\npkgdesc={desc}\narch=('x86_64' 'aarch64')\nurl={url}\nlicense=({lic})\ndepends=({deps})\n{opt}makedepends=({mk})\nprovides=({name})\nconflicts=({name} {name}-bin)\noptions=('!lto')\nsource=(\"{repo_name}::git+{repo}.git\")\nsha256sums=('SKIP')\n\n\
         pkgver() {{\n    cd \"$srcdir/{repo_name}\"\n    local v\n    v=$(git describe --tags --abbrev=0 2>/dev/null || echo v{ver})\n    printf '%s.r%s.g%s' \"${{v#v}}\" \"$(git rev-list --count HEAD)\" \"$(git rev-parse --short HEAD)\"\n}}\n\n\
         prepare() {{\n    cd {cd}\n    export RUSTUP_TOOLCHAIN=stable\n    cargo fetch --target \"$(rustc -vV | sed -n 's/host: //p')\"\n}}\n\n\
         build() {{\n    cd {cd}\n    export RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR=\"$srcdir/target\"\n{features_sel}    cargo build --release --offline -p {krate} --bin {bin} \"${{_f[@]}}\"\n}}\n\n\
         package() {{\n    cd {cd}\n    CARGO_TARGET_DIR=\"$srcdir/target\"\n{inst}}}\n",
        ver = app.version.replace('-', "_"),
        desc = sh(&desc_en(app)),
        url = sh(&repo),
        lic = sh(&license(app)),
        mk = arr(&mk),
        krate = app.krate,
    );

    // -bin: суммы готовых архивов, если они собраны
    let sum = |t: Target| -> String {
        let f = app.dist_dir().join(format!("{}.sha256", tar_name(app, t).unwrap_or_default()));
        std::fs::read_to_string(f).ok().and_then(|s| s.split_whitespace().next().map(str::to_string)).unwrap_or_else(|| "SKIP".into())
    };
    let (sx, sa) = (sum(Target::LinuxX86_64), sum(Target::LinuxAarch64));
    if sx == "SKIP" || sa == "SKIP" {
        util::warn("для -bin нет сумм архивов (syngui package --format tar --target linux-x86_64 и linux-aarch64 --release) — стоит SKIP");
    }
    let binpkg = format!(
        "{maint}pkgname={name}-bin\npkgver={ver}\npkgrel=1\npkgdesc={desc}\narch=('x86_64' 'aarch64')\nurl={url}\nlicense=({lic})\ndepends=({deps})\n{opt}provides=({name})\nconflicts=({name} {name}-git)\noptions=('!strip' '!debug')\n\
         source_x86_64=(\"{repo}/releases/download/v$pkgver/{bin}-$pkgver-x86_64.tar.zst\")\nsource_aarch64=(\"{repo}/releases/download/v$pkgver/{bin}-$pkgver-aarch64.tar.zst\")\n\
         sha256sums_x86_64=('{sx}')\nsha256sums_aarch64=('{sa}')\n\npackage() {{\n    cd \"$srcdir\"\n{INSTALL_TREE}}}\n",
        ver = app.version,
        desc = sh(&desc_en(app)),
        url = sh(&repo),
        lic = sh(&license(app)),
    );

    for (suffix, text) in [("git", git), ("bin", binpkg)] {
        let d = out.join(format!("{name}-{suffix}"));
        util::write(&d.join("PKGBUILD"), &text)?;
        let mut c = Command::new("makepkg");
        c.current_dir(&d).arg("--printsrcinfo");
        let info = util::capture(c).with_context(|| format!("makepkg --printsrcinfo для {name}-{suffix}"))?;
        util::write(&d.join(".SRCINFO"), info)?;
        util::note(format!("{}", d.display()));
    }
    if !repo.contains("github.com") {
        bail!("-bin рассчитан на релизы GitHub ({repo}/releases/download/…) — поправьте source вручную");
    }
    Ok(out)
}
