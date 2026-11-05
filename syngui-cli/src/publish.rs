//! `syngui release <версия>` — выпуск: версия в Cargo.toml, коммит, тег; по `--publish` — артефакты,
//! push, релиз GitHub и AUR. Без флагов — сухой прогон: только показать, что будет сделано.

use crate::package;
use crate::project::App;
use crate::release::Release;
use crate::target::{Profile, Target};
use crate::{android, ios, util};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Opts<'a> {
    pub version: &'a str,
    pub manifest: Option<&'a Path>,
    pub package: Option<&'a str>,
    pub apply: bool,
    pub publish: bool,
    pub no_mobile: bool,
    pub release_file: Option<&'a Path>,
    /// Внешние готовые файлы (каталог — его содержимое).
    pub assets: Vec<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Art {
    Tar(Target),
    TarPortable,
    Zip,
    Apk,
    Ipa,
}

fn parse_arts(v: &[String]) -> Result<Vec<Art>> {
    let mut out = Vec::new();
    for a in v {
        match a.as_str() {
            "tar" => out.extend([Art::Tar(Target::LinuxX86_64), Art::Tar(Target::LinuxAarch64)]),
            "tar-x86_64" => out.push(Art::Tar(Target::LinuxX86_64)),
            "tar-aarch64" => out.push(Art::Tar(Target::LinuxAarch64)),
            "tar-portable" => out.push(Art::TarPortable),
            "zip" => out.push(Art::Zip),
            "apk" => out.push(Art::Apk),
            "ipa" => out.push(Art::Ipa),
            _ => bail!("release.artifacts: неизвестное «{a}» — tar, tar-x86_64, tar-aarch64, tar-portable, zip, apk, ipa"),
        }
    }
    Ok(out)
}

/// Раскрыть маски/пути внешних файлов: файлы — как есть, каталоги — их содержимое
/// (из CLI) или сами (внутри маски → архив <имя>.tar).
fn expand_assets(patterns: &[String], base: &Path, cli: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut items = Vec::new();
    for p in patterns {
        let pat = util::expand(p);
        let pat = if pat.is_absolute() { pat } else { base.join(pat) };
        let mut c = Command::new("bash");
        c.args(["-c", "shopt -s nullglob; for f in $1; do printf '%s\\0' \"$f\"; done", "_"]).arg(pat.display().to_string());
        let out = util::capture(c)?;
        let found: Vec<PathBuf> = out.split('\0').filter(|s| !s.is_empty()).map(PathBuf::from).collect();
        if found.is_empty() {
            util::warn(format!("release.assets: по «{p}» ничего не найдено"));
        }
        items.extend(found);
    }
    for d in cli {
        if d.is_dir() {
            let mut v: Vec<PathBuf> = std::fs::read_dir(d)?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            v.sort();
            items.extend(v);
        } else if d.is_file() {
            items.push(d.clone());
        } else {
            bail!("--assets: нет {}", d.display());
        }
    }
    Ok(items)
}

fn human(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.2} ГБ", n as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.1} МБ", n as f64 / (1u64 << 20) as f64)
    }
}

#[derive(serde::Deserialize)]
struct Meta {
    packages: Vec<MPkg>,
    workspace_members: Vec<String>,
    workspace_root: PathBuf,
    target_directory: PathBuf,
}
#[derive(serde::Deserialize)]
struct MPkg {
    id: String,
    name: String,
    metadata: Option<serde_json::Value>,
}

fn git(root: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(root).args(args);
    c
}

/// Поднять версию: [workspace.package] version, иначе [package] version — в корневом Cargo.toml
/// (или в Cargo.toml крейта, если у него своя версия). Вернуть изменённый файл.
fn bump(cargo_toml: &Path, version: &str, dry: bool) -> Result<Option<String>> {
    let text = std::fs::read_to_string(cargo_toml)?;
    let mut section = String::new();
    let mut out = Vec::new();
    let mut old = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.trim_matches(|c| c == '[' || c == ']').trim().to_string();
        }
        if old.is_none() && (section == "workspace.package" || section == "package") && t.starts_with("version") && t.contains('"') && !t.contains("workspace")
        {
            let cur = t.split('"').nth(1).unwrap_or_default().to_string();
            old = Some(cur);
            let indent = &line[..line.len() - line.trim_start().len()];
            out.push(format!("{indent}version = \"{version}\""));
            continue;
        }
        out.push(line.to_string());
    }
    if old.is_some() && !dry {
        let mut s = out.join("\n");
        if text.ends_with('\n') {
            s.push('\n');
        }
        std::fs::write(cargo_toml, s)?;
    }
    Ok(old)
}

pub fn run(o: Opts) -> Result<()> {
    let v = o.version.trim_start_matches('v');
    if !v.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) || v.split('.').count() != 3 {
        bail!("версия вида 0.2.0 (получено {})", o.version);
    }
    let dry = !o.apply && !o.publish;
    let mut c = Command::new("cargo");
    c.args(["metadata", "--format-version", "1", "--no-deps"]);
    if let Some(m) = o.manifest {
        c.arg("--manifest-path").arg(m);
    }
    let m: Meta = serde_json::from_str(&util::capture(c)?)?;
    let root = m.workspace_root.clone();
    let release_file = o.release_file.map(Path::to_path_buf).unwrap_or_else(|| root.join("syngui-release.toml"));
    let workspace = release_file.is_file() && o.package.is_none();
    let tag = format!("v{v}");

    // Где версия: корень (workspace.package) или крейт
    let root_toml = root.join("Cargo.toml");
    let app = if workspace { None } else { Some(App::load(o.manifest, o.package)?) };
    let version_file = match &app {
        Some(a) if bump(&a.manifest_dir.join("Cargo.toml"), v, true)?.is_some() => a.manifest_dir.join("Cargo.toml"),
        _ => root_toml.clone(),
    };
    let cur = bump(&version_file, v, true)?.with_context(|| format!("в {} нет version в [workspace.package] или [package]", version_file.display()))?;

    // Мобильные артефакты: крейты с разделами android / ios в метаданных
    let mut mobile: Vec<(String, bool, bool)> = Vec::new();
    if !o.no_mobile {
        for p in m.packages.iter().filter(|p| m.workspace_members.contains(&p.id)) {
            if !workspace && app.as_ref().is_some_and(|a| a.krate != p.name) {
                continue;
            }
            let s = p.metadata.as_ref().and_then(|x| x.get("syngui"));
            let a = s.and_then(|s| s.get("android")).is_some() || p.metadata.as_ref().and_then(|x| x.get("android")).is_some();
            let i = s.and_then(|s| s.get("ios")).is_some();
            if a || i {
                mobile.push((p.name.clone(), a, i));
            }
        }
    }

    let mut dirty = util::capture(git(&root, &["status", "--porcelain", "--untracked-files=no"]))?;
    dirty = dirty.trim().to_string();
    let tag_exists = git(&root, &["rev-parse", "-q", "--verify", &format!("refs/tags/{tag}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let repo = git(&root, &["remote", "get-url", "origin"])
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    util::step(format!(
        "выпуск {tag} ({}){}",
        if workspace {
            "рабочее пространство по syngui-release.toml"
        } else {
            "приложение"
        },
        if dry { " — сухой прогон" } else { "" }
    ));
    println!("  версия: {cur} → {v} в {}", version_file.display());
    println!("  коммит «Выпуск {v}» (Cargo.toml, Cargo.lock) и тег {tag}");
    // Что собрать: release.artifacts приложения или tar обеих архитектур (+ APK/IPA по метаданным)
    let arts: Option<Vec<Art>> = match app.as_ref().and_then(|a| a.meta.release.artifacts.clone()) {
        Some(v) => Some(parse_arts(&v)?),
        None => None,
    };
    if let Some(a) = &arts {
        // явный список заменяет автоопределение APK/IPA
        let krate = app.as_ref().unwrap().krate.clone();
        mobile = if o.no_mobile { vec![] } else { vec![(krate, a.contains(&Art::Apk), a.contains(&Art::Ipa))] };
        mobile.retain(|(_, a, i)| *a || *i);
    }
    let asset_patterns: Vec<String> = if workspace {
        Release::load(o.manifest, Some(&release_file)).map(|r| r.spec.assets).unwrap_or_default()
    } else {
        app.as_ref().map(|a| a.meta.release.assets.clone()).unwrap_or_default()
    };
    let asset_base = app.as_ref().map(|a| a.manifest_dir.clone()).unwrap_or_else(|| root.clone());
    let assets = expand_assets(&asset_patterns, &asset_base, &o.assets)?;
    if o.publish || dry {
        let what = match &arts {
            Some(a) => a
                .iter()
                .filter(|x| !matches!(x, Art::Apk | Art::Ipa))
                .map(|x| match x {
                    Art::Tar(t) => format!("tar {t}"),
                    Art::TarPortable => "tar linux-x86_64 --portable".into(),
                    Art::Zip => "zip windows-x86_64".into(),
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>()
                .join(", "),
            None if workspace => "-w -f tar -t all-linux --release".to_string(),
            None => "-f tar -t all-linux --release".to_string(),
        };
        println!("  {}: {what}", if dry { "при --publish собрать" } else { "сборка" });
        for a in &assets {
            let size = if a.is_dir() { "каталог → .tar".to_string() } else { std::fs::metadata(a).map(|m| human(m.len())).unwrap_or_default() };
            println!("      + {} ({size})", a.display());
        }
        for (n, a, i) in &mobile {
            if *a {
                println!(
                    "      APK {n} (релизный ключ {})",
                    if has_key(&root, n, o.manifest) {
                        "есть"
                    } else {
                        "НЕТ — будет пропущен"
                    }
                );
            }
            if *i {
                println!("      IPA {n}");
            }
        }
        println!("      git push origin HEAD {tag} ({})", if repo.is_empty() { "нет origin" } else { &repo });
        println!("      gh release create {tag}: архивы, APK/IPA, SHA256SUMS");
        println!(
            "      AUR: syngui package {}-f aur с суммами релиза (не публикуется — git push в AUR делает человек)",
            if workspace { "-w " } else { "" }
        );
    }
    if !dirty.is_empty() {
        let msg = format!("в рабочей копии незакоммиченные изменения:\n{dirty}");
        if dry {
            util::warn(msg);
        } else {
            bail!("{msg}");
        }
    }
    // Уже выпущено `--apply` (тег на HEAD, версия та же) — публикуем без нового коммита
    let tag_at_head = tag_exists && {
        let t = util::capture(git(&root, &["rev-list", "-n1", &tag])).unwrap_or_default();
        let h = util::capture(git(&root, &["rev-parse", "HEAD"])).unwrap_or_default();
        t.trim() == h.trim()
    };
    let already = tag_at_head && cur == v;
    if already {
        println!("  {tag} уже стоит на HEAD (версия {v}) — коммит и тег не нужны");
    }
    if tag_exists && !already {
        let msg = format!("тег {tag} уже есть");
        if dry {
            util::warn(msg);
        } else {
            bail!("{msg}");
        }
    }
    if repo.is_empty() && (o.publish || dry) {
        let msg = "у репозитория нет remote origin — для --publish (git push, gh release) его нужно завести";
        if dry {
            util::warn(msg);
        } else {
            bail!("{msg}");
        }
    }
    if dry {
        println!("\nСухой прогон: ничего не изменено. --apply — версия, коммит и тег; --publish — ещё сборка, push, релиз GitHub и AUR.");
        return Ok(());
    }

    // Версия, Cargo.lock, коммит, тег
    if already && !o.publish {
        println!("Уже выпущено локально: {tag}.");
        return Ok(());
    }
    if !already {
        bump(&version_file, v, false)?;
        let mut c = Command::new("cargo");
        c.current_dir(&root)
            .args(["metadata", "--format-version", "1"])
            .stdout(std::process::Stdio::null());
        util::run(c).context("обновление Cargo.lock")?;
        let mut add = vec!["add", "--"];
        let vf = version_file.strip_prefix(&root).unwrap_or(&version_file).display().to_string();
        add.push(&vf);
        let tracked_lock = git(&root, &["ls-files", "--error-unmatch", "Cargo.lock"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if tracked_lock {
            add.push("Cargo.lock");
        }
        util::run(git(&root, &add))?;
        util::run(git(&root, &["commit", "-q", "-m", &format!("Выпуск {v}")]))?;
        util::run(git(&root, &["tag", "-a", &tag, "-m", &format!("{tag}")]))?;
        util::step(format!("коммит «Выпуск {v}» и тег {tag}"));
    }
    if !o.publish {
        println!("Готово локально. Опубликовать: syngui release {v} --publish (сборка, push, релиз GitHub, AUR).");
        return Ok(());
    }

    // Артефакты в отдельный каталог релиза
    let out_dir = m.target_directory.join("syngui/releases").join(&tag);
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    std::fs::create_dir_all(&out_dir)?;
    let mut files: Vec<PathBuf> = Vec::new();
    if let Some(arts) = &arts {
        // перечитать: версия уже поднята
        let a = &App::load(o.manifest, o.package)?;
        for x in arts {
            match x {
                Art::Tar(t) => files.push(package::tar(a, *t, Profile::Release, false)?),
                Art::TarPortable => files.push(package::tar(a, Target::LinuxX86_64, Profile::Release, true)?),
                Art::Zip => files.push(package::zip(a, Profile::Release)?),
                Art::Apk | Art::Ipa => {}
            }
        }
    } else if workspace {
        let r = Release::load(o.manifest, Some(&release_file))?;
        for t in [Target::LinuxX86_64, Target::LinuxAarch64] {
            files.extend(r.tars(&[], t, Profile::Release, true)?);
        }
    } else {
        let a = App::load(o.manifest, o.package)?;
        for t in [Target::LinuxX86_64, Target::LinuxAarch64] {
            files.push(package::tar(&a, t, Profile::Release, false)?);
        }
    }
    for (n, a, i) in &mobile {
        let app = App::load(o.manifest, Some(n))?;
        if *a {
            if has_key(&root, n, o.manifest) {
                files.push(android::build(&app, Profile::Release, true, None)?);
            } else {
                util::warn(format!("APK {n} пропущен: нет релизного ключа (syngui keygen -p {n})"));
            }
        }
        if *i {
            files.push(ios::build(&app, Profile::Release)?);
        }
    }
    for f in &files {
        util::copy(f, &out_dir.join(f.file_name().unwrap()))?;
    }
    let mut upload: Vec<PathBuf> = files.iter().map(|f| out_dir.join(f.file_name().unwrap())).collect();
    // Внешние файлы: как есть (не копируются — образы большие), каталоги — архивом <имя>.tar
    for a in &assets {
        let name = a.file_name().unwrap().to_string_lossy().into_owned();
        if name == "SHA256SUMS" {
            util::note(format!("{}: заменяется общим SHA256SUMS релиза (в нём суммы всех файлов)", a.display()));
            continue;
        }
        let item = if a.is_dir() {
            let t = out_dir.join(format!("{name}.tar"));
            let mut c = Command::new("tar");
            c.args(["--owner=0", "--group=0", "--numeric-owner", "-cf"]).arg(&t).arg("-C").arg(a.parent().unwrap()).arg(&name);
            util::run(c)?;
            t
        } else {
            a.clone()
        };
        let n = item.file_name().unwrap().to_string_lossy().into_owned();
        if upload.iter().any(|u| u.file_name().unwrap().to_string_lossy() == n) {
            bail!("в релизе два файла с именем {n}");
        }
        upload.push(item);
    }
    // Общий SHA256SUMS по всем файлам релиза (имена — как у файлов в релизе)
    let mut c = Command::new("sha256sum");
    c.args(&upload);
    let sums_text: String = util::capture(c)?
        .lines()
        .map(|l| match l.split_once("  ") {
            Some((h, p)) => format!("{h}  {}\n", Path::new(p).file_name().unwrap().to_string_lossy()),
            None => format!("{l}\n"),
        })
        .collect();
    let sums = out_dir.join("SHA256SUMS");
    util::write(&sums, sums_text)?;
    upload.push(sums);
    let total: u64 = upload.iter().filter_map(|u| std::fs::metadata(u).ok()).map(|m| m.len()).sum();
    if let Some(big) = upload.iter().find(|u| std::fs::metadata(u).map(|m| m.len() >= 2 << 30).unwrap_or(false)) {
        bail!("{}: файл больше 2 ГиБ — GitHub его не примет", big.display());
    }
    util::note(format!("в релиз: {} файлов, {}", upload.len(), human(total)));

    util::step(format!("git push origin HEAD {tag}"));
    util::run(git(&root, &["push", "origin", "HEAD", &tag]))?;
    util::step(format!("gh release create {tag} ({} файлов)", upload.len()));
    let mut c = Command::new("gh");
    c.current_dir(&root)
        .args([
            "release",
            "create",
            &tag,
            "--title",
            &tag,
            "--notes",
            &format!("Выпуск {v}. Суммы — SHA256SUMS."),
        ])
        .args(&upload);
    util::run(c)?;

    let aur_dir = m.target_directory.join("syngui/aur");
    if workspace {
        Release::load(o.manifest, Some(&release_file))?.aur(&[], &aur_dir)?;
    } else {
        package::aur(&App::load(o.manifest, o.package)?, Some(&aur_dir))?;
    }
    println!(
        "Релиз {tag} опубликован на GitHub. AUR: {} — проверить и выложить (git push в AUR) вручную.",
        aur_dir.display()
    );
    Ok(())
}

fn has_key(_root: &Path, krate: &str, manifest: Option<&Path>) -> bool {
    App::load(manifest, Some(krate))
        .map(|a| {
            let d = android::key_dir(&a);
            d.join("release.jks").is_file() && d.join("password").is_file()
        })
        .unwrap_or(false)
}
