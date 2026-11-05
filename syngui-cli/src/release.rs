//! Выпуск рабочего пространства из нескольких пакетов (synshell): описание — `syngui-release.toml`
//! в корне (или путь в `[workspace.metadata.syngui] release`).
//!
//! Из описания генерируются bash-функции `package_<имя>()`; одни и те же функции работают в
//! локальном split-PKGBUILD (makepkg), в AUR `<имя>-git` и при упаковке архивов для `<имя>-bin` —
//! пути расходиться не могут.
//!
//! ```toml
//! name = "synshell"                      # pkgbase
//! url = "https://github.com/VitaminDB/synshell"
//! license = ["GPL-3.0-only"]
//! makedepends = ["cargo", "git", "clang", "pkgconf"]
//!
//! [crates.synaptixd]                     # особая сборка крейта
//! features = ["cuda", "npu"]
//! features_env = "SYNAPTIXD_FEATURES"    # переопределение: SYNAPTIXD_FEATURES=cuda
//!
//! [packages.synshell-core]
//! description = "…"
//! description_en = "…"
//! depends = ["mesa"]
//! optdepends = ["xorg-xwayland: X11-программы"]
//! optdepends_en = ["xorg-xwayland: X11 applications"]
//! bins = ["synwm", "synfsd"]              # программы → /usr/bin
//! apps = ["synsettings"]                 # программа + ярлык + значок (data/ её крейта)
//! desktop = ["synshot"]                  # только ярлык + значок
//! files = [
//!   { from = "data/synshell-session", to = "usr/bin/", mode = "755" },
//!   { from = "data/sounds/themes/*", to = "usr/share/sounds/" },
//!   { run = "echo '[Default Applications]'; cat crates/*/data/defaults.mimeapps | grep '='", to = "usr/share/applications/synshell-mimeapps.list" },
//! ]
//! links = { "usr/bin/syndesktop" = "synwm" }
//! dirs = ["usr/share/synaptix/models"]
//! provides = ["syndesktop"]; conflicts = ["syndesktop"]; replaces = ["syndesktop"]
//! backup = ["etc/synaptix/synaptixd.toml"]; groups = ["synshell-apps"]
//! install = "packaging/synshell-core.install"
//! ```

use crate::linux;
use crate::target::{Profile, Target};
use crate::util::{self, sh};
use anyhow::{bail, Context, Result};
use indexmap_like::Map;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

mod indexmap_like {
    /// Таблица TOML с сохранённым порядком (toml/preserve_order).
    pub type Map<V> = toml::map::Map<String, V>;
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub name: String,
    pub version: Option<String>,
    pub pkgrel: Option<u32>,
    pub url: Option<String>,
    #[serde(default)]
    pub license: Vec<String>,
    pub maintainer: Option<String>,
    #[serde(default)]
    pub makedepends: Vec<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub crates: BTreeMap<String, CrateSpec>,
    /// Готовые внешние файлы для релиза GitHub (`syngui release --publish`): пути или маски.
    #[serde(default)]
    pub assets: Vec<String>,
    pub packages: Map<toml::Value>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct CrateSpec {
    pub features: Vec<String>,
    pub features_env: Option<String>,
    pub no_default_features: bool,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Pkg {
    pub description: String,
    pub description_en: Option<String>,
    pub depends: Vec<String>,
    pub optdepends: Vec<String>,
    pub optdepends_en: Vec<String>,
    pub makedepends: Vec<String>,
    pub bins: Vec<String>,
    pub apps: Vec<String>,
    pub desktop: Vec<String>,
    pub files: Vec<FileSpec>,
    pub links: BTreeMap<String, String>,
    pub dirs: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub backup: Vec<String>,
    pub groups: Vec<String>,
    pub install: Option<String>,
    /// Публиковать в AUR (по умолчанию да).
    pub aur: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct FileSpec {
    /// Путь или маска от корня рабочего пространства; каталог копируется целиком.
    pub from: Option<String>,
    /// Команда bash (в корне рабочего пространства), её вывод — содержимое файла.
    pub run: Option<String>,
    /// Куда: `…/` — в каталог (имя сохраняется), иначе — точный путь. `{pkgname}` — имя пакета.
    pub to: String,
    pub mode: Option<String>,
}

impl Pkg {
    fn all_bins(&self) -> impl Iterator<Item = &String> {
        self.bins.iter().chain(self.apps.iter())
    }
    fn is_meta(&self) -> bool {
        self.bins.is_empty() && self.apps.is_empty() && self.desktop.is_empty() && self.files.is_empty() && self.links.is_empty() && self.dirs.is_empty()
    }
}

/// Рабочее пространство: программы и их крейты.
pub struct Release {
    pub spec: Spec,
    pub packages: Vec<(String, Pkg)>,
    pub root: PathBuf,
    pub target_dir: PathBuf,
    pub version: String,
    /// программа → (крейт, каталог крейта относительно корня)
    pub bin_crate: BTreeMap<String, (String, String)>,
}

#[derive(Deserialize)]
struct Meta {
    packages: Vec<MPkg>,
    workspace_members: Vec<String>,
    workspace_root: PathBuf,
    target_directory: PathBuf,
    metadata: Option<serde_json::Value>,
}
#[derive(Deserialize)]
struct MPkg {
    id: String,
    name: String,
    manifest_path: PathBuf,
    targets: Vec<MTarget>,
}
#[derive(Deserialize)]
struct MTarget {
    name: String,
    kind: Vec<String>,
}

impl Release {
    pub fn load(manifest: Option<&Path>, file: Option<&Path>) -> Result<Release> {
        let mut c = Command::new("cargo");
        c.args(["metadata", "--format-version", "1", "--no-deps"]);
        if let Some(m) = manifest {
            c.arg("--manifest-path").arg(m);
        }
        let m: Meta = serde_json::from_str(&util::capture(c)?).context("разбор cargo metadata")?;
        let root = m.workspace_root.clone();
        let path = match file {
            Some(f) => f.to_path_buf(),
            None => match m.metadata.as_ref().and_then(|v| v.pointer("/syngui/release")).and_then(|v| v.as_str()) {
                Some(p) => root.join(p),
                None => root.join("syngui-release.toml"),
            },
        };
        let text = std::fs::read_to_string(&path).with_context(|| format!("нет описания выпуска {} (syngui-release.toml в корне рабочего пространства)", path.display()))?;
        let spec: Spec = toml::from_str(&text).with_context(|| format!("разбор {}", path.display()))?;
        let mut packages = Vec::new();
        for (name, v) in &spec.packages {
            let p: Pkg = v.clone().try_into().with_context(|| format!("[packages.{name}] в {}", path.display()))?;
            packages.push((name.clone(), p));
        }
        let mut bin_crate = BTreeMap::new();
        for p in m.packages.iter().filter(|p| m.workspace_members.contains(&p.id)) {
            let dir = p.manifest_path.parent().unwrap().strip_prefix(&root).unwrap_or(Path::new("")).to_string_lossy().into_owned();
            for t in p.targets.iter().filter(|t| t.kind.iter().any(|k| k == "bin")) {
                bin_crate.insert(t.name.clone(), (p.name.clone(), dir.clone()));
            }
        }
        for (name, p) in &packages {
            for b in p.all_bins().chain(p.desktop.iter()) {
                if !bin_crate.contains_key(b) {
                    bail!("[packages.{name}]: программы {b} нет в рабочем пространстве (bin крейтов {})", root.display());
                }
            }
        }
        let version = match &spec.version {
            Some(v) => v.clone(),
            None => workspace_version(&root)?,
        };
        Ok(Release { spec, packages, root, target_dir: m.target_directory, version, bin_crate })
    }

    fn pkgver(&self) -> String {
        self.version.replace('-', "_")
    }

    fn pkgrel(&self) -> u32 {
        self.spec.pkgrel.unwrap_or_else(|| {
            let mut c = Command::new("git");
            c.arg("-C").arg(&self.root).args(["rev-list", "--count", "HEAD"]).stderr(std::process::Stdio::null());
            c.output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(1)
        })
    }

    /// Выбранные пакеты (`--only a,b`) или все.
    fn selected(&self, only: &[String]) -> Result<Vec<&(String, Pkg)>> {
        if only.is_empty() {
            return Ok(self.packages.iter().collect());
        }
        only.iter().map(|n| self.packages.iter().find(|(p, _)| p == n).with_context(|| format!("нет пакета {n} в описании выпуска"))).collect()
    }

    /// Команды cargo build: общая для крейтов без особенностей и отдельные — с возможностями.
    fn cargo_plans(&self, pkgs: &[&(String, Pkg)]) -> Vec<(Vec<String>, Vec<String>)> {
        let mut plain: Vec<String> = Vec::new();
        let mut special: Vec<(Vec<String>, Vec<String>)> = Vec::new();
        for (_, p) in pkgs {
            for b in p.all_bins() {
                let krate = &self.bin_crate[b].0;
                if let Some(cs) = self.spec.crates.get(krate) {
                    if special.iter().any(|(ps, _)| ps.contains(krate)) {
                        continue;
                    }
                    let feats: Vec<String> = match cs.features_env.as_deref().and_then(|e| std::env::var(e).ok()) {
                        Some(v) => v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect(),
                        None => cs.features.clone(),
                    };
                    let mut args = vec!["-p".to_string(), krate.clone(), "--bins".into()];
                    if cs.no_default_features {
                        args.push("--no-default-features".into());
                    }
                    if !feats.is_empty() {
                        args.push("--features".into());
                        args.push(feats.iter().map(|f| if f.contains('/') { f.clone() } else { format!("{krate}/{f}") }).collect::<Vec<_>>().join(","));
                    }
                    special.push((vec![krate.clone()], args));
                } else if !plain.contains(krate) {
                    plain.push(krate.clone());
                }
            }
        }
        let mut out = Vec::new();
        if !plain.is_empty() {
            out.push((plain.clone(), plain.iter().flat_map(|c| ["-p".to_string(), c.clone()]).collect()));
        }
        out.extend(special);
        out
    }

    /// Собрать все программы выбранных пакетов; вернуть каталог с ними (после strip).
    fn build(&self, pkgs: &[&(String, Pkg)], target: Target, profile: Profile, strip: bool) -> Result<PathBuf> {
        for (_, args) in self.cargo_plans(pkgs) {
            let mut c = Command::new("cargo");
            c.current_dir(&self.root).arg("build");
            if profile == Profile::Fast && !linux::has_fast_release(&self.root) {
                for kv in ["inherits=\"release\"", "lto=false", "codegen-units=256", "incremental=true", "debug=false"] {
                    c.arg("--config").arg(format!("profile.fast-release.{kv}"));
                }
            }
            c.args(profile.cargo_args()).args(&args);
            if !target.is_host() {
                c.arg("--target").arg(target.triple());
                if target == Target::LinuxAarch64 {
                    linux::cross_env(&self.target_dir, &mut c)?;
                }
            }
            util::step(format!("cargo build ({target}, {}): {}", profile.dir(), args.iter().filter(|a| !a.starts_with('-')).cloned().collect::<Vec<_>>().join(" ")));
            util::run(c)?;
        }
        let out = if target.is_host() { self.target_dir.join(profile.dir()) } else { self.target_dir.join(target.triple()).join(profile.dir()) };
        // Копии программ (strip не трогает target/ — инкрементальная сборка цела)
        let bins = self.target_dir.join("syngui/release").join(&self.spec.name).join(target.arch()).join("bin");
        std::fs::create_dir_all(&bins)?;
        let strip_tool = util::which("llvm-strip");
        for (_, p) in pkgs {
            for b in p.all_bins() {
                let src = out.join(b);
                if !src.is_file() {
                    bail!("cargo не оставил программу {}", src.display());
                }
                let dst = bins.join(b);
                let _ = std::fs::remove_file(&dst);
                std::fs::copy(&src, &dst)?;
                if strip {
                    if let Some(s) = &strip_tool {
                        let mut c = Command::new(s);
                        c.arg("--strip-unneeded").arg(&dst);
                        util::run(c)?;
                    }
                }
            }
        }
        Ok(bins)
    }

    /// Тело package_<имя>(): установка файлов из `$_root` (корень) и `$_bins` (программы).
    pub fn package_body(&self, name: &str, p: &Pkg) -> Result<String> {
        let mut s = String::from("    cd \"$_root\"\n");
        for b in p.all_bins() {
            s += &format!("    install -Dm755 \"$_bins/{b}\" \"$pkgdir/usr/bin/{b}\"\n");
        }
        for b in p.apps.iter().chain(p.desktop.iter()) {
            let dir = &self.bin_crate[b].1;
            let data = if dir.is_empty() { "data".to_string() } else { format!("{dir}/data") };
            let mut any = false;
            for (rel, to) in [
                (format!("{data}/{b}.desktop"), format!("usr/share/applications/{b}.desktop")),
                (format!("{data}/icons/{b}.svg"), format!("usr/share/icons/hicolor/scalable/apps/{b}.svg")),
                (format!("{data}/icons/{b}.png"), format!("usr/share/icons/hicolor/256x256/apps/{b}.png")),
            ] {
                if self.root.join(&rel).is_file() {
                    s += &format!("    install -Dm644 {} \"$pkgdir/{to}\"\n", sh(&rel));
                    any |= rel.ends_with(".desktop");
                }
            }
            if !any {
                util::warn(format!("[packages.{name}]: у {b} нет ярлыка {data}/{b}.desktop"));
            }
        }
        for d in &p.dirs {
            s += &format!("    install -dm755 \"$pkgdir/{}\"\n", d.trim_start_matches('/'));
        }
        for f in &p.files {
            let to = f.to.trim_start_matches('/').replace("{pkgname}", name);
            let mode = f.mode.clone().unwrap_or_else(|| "644".into());
            match (&f.from, &f.run) {
                (Some(from), None) => {
                    let glob = from.contains(['*', '?', '[']);
                    if to.ends_with('/') {
                        // каталог: файлы по маске — внутрь, подкаталоги — целиком
                        let pat = if glob { from.clone() } else { sh(from) };
                        s += &format!(
                            "    install -dm755 \"$pkgdir/{to}\"\n    for _f in {pat}; do\n        [ -e \"$_f\" ] || {{ echo \"нет {from}\" >&2; return 1; }}\n        if [ -d \"$_f\" ]; then cp -r --no-preserve=ownership \"$_f\" \"$pkgdir/{to}\"; else install -m{mode} \"$_f\" \"$pkgdir/{to}\"; fi\n    done\n"
                        );
                    } else {
                        if glob {
                            bail!("[packages.{name}] files: маска {from} — только в каталог (to = \"…/\")");
                        }
                        if self.root.join(from).is_dir() {
                            s += &format!("    install -dm755 \"$pkgdir/{to}\"\n    cp -r --no-preserve=ownership {}/. \"$pkgdir/{to}/\"\n", sh(from));
                        } else {
                            s += &format!("    install -Dm{mode} {} \"$pkgdir/{to}\"\n", sh(from));
                        }
                    }
                }
                (None, Some(run)) => {
                    s += &format!("    ( {run} ) | install -Dm{mode} /dev/stdin \"$pkgdir/{to}\"\n");
                }
                _ => bail!("[packages.{name}] files: нужно одно из from / run (to = {})", f.to),
            }
        }
        for (link, target) in &p.links {
            let link = link.trim_start_matches('/');
            let parent = Path::new(link).parent().map(|p| p.display().to_string()).unwrap_or_default();
            s += &format!("    install -dm755 \"$pkgdir/{parent}\"\n    ln -sf {} \"$pkgdir/{link}\"\n", sh(target));
        }
        Ok(s)
    }

    /// Метаданные пакета (внутри package_<имя>() или в начале одиночного PKGBUILD).
    fn pkg_meta(p: &Pkg, en: bool, indent: &str, depends: &[String], extra_provides: &[String], extra_conflicts: &[String], install: bool) -> String {
        let mut s = String::new();
        let desc = if en { p.description_en.clone().unwrap_or_else(|| p.description.clone()) } else { p.description.clone() };
        s += &format!("{indent}pkgdesc={}\n", sh(&desc));
        let a = |k: &str, v: &[String]| if v.is_empty() { String::new() } else { format!("{indent}{k}=({})\n", v.iter().map(|x| sh(x)).collect::<Vec<_>>().join(" ")) };
        s += &a("depends", depends);
        let opt = if en && !p.optdepends_en.is_empty() { &p.optdepends_en } else { &p.optdepends };
        if !opt.is_empty() {
            s += &format!("{indent}optdepends=({})\n", opt.iter().map(|x| sh(x)).collect::<Vec<_>>().join(&format!("\n{indent}            ")));
        }
        let provides: Vec<String> = extra_provides.iter().chain(p.provides.iter()).cloned().collect();
        let conflicts: Vec<String> = extra_conflicts.iter().chain(p.conflicts.iter()).cloned().collect();
        s += &a("provides", &provides);
        s += &a("conflicts", &conflicts);
        s += &a("replaces", &p.replaces);
        s += &a("backup", &p.backup);
        s += &a("groups", &p.groups);
        if install {
            if let Some(i) = &p.install {
                s += &format!("{indent}install={}\n", Path::new(i).file_name().unwrap().to_string_lossy());
            }
        }
        s
    }

    fn license(&self) -> String {
        let l = if self.spec.license.is_empty() { vec!["GPL-3.0-only".to_string()] } else { self.spec.license.clone() };
        l.iter().map(|x| sh(x)).collect::<Vec<_>>().join(" ")
    }

    /// Локальный split-PKGBUILD + makepkg → пакеты в dist; вернуть их пути (и каталог pkg/ makepkg).
    pub fn arch(&self, only: &[String], target: Target, profile: Profile, strip: bool) -> Result<(Vec<PathBuf>, PathBuf)> {
        let pkgs = self.selected(only)?;
        let bins = self.build(&pkgs, target, profile, strip)?;
        let dir = self.target_dir.join("syngui/release").join(&self.spec.name).join(target.arch()).join("makepkg");
        std::fs::create_dir_all(&dir)?;
        let names: Vec<&str> = pkgs.iter().map(|(n, _)| n.as_str()).collect();
        let mut s = format!(
            "# Сгенерировано syngui package --workspace из syngui-release.toml — правки делать там.\npkgbase={}\npkgname=({})\npkgver={}\npkgrel={}\npkgdesc={}\narch=({})\nurl={}\nlicense=({})\noptions=('!strip' '!debug' '!lto')\n\n_root={}\n_bins={}\n",
            self.spec.name,
            names.join(" "),
            self.pkgver(),
            self.pkgrel(),
            sh(&pkgs[0].1.description),
            sh(target.arch()),
            sh(self.spec.url.as_deref().unwrap_or("")),
            self.license(),
            sh(&self.root.display().to_string()),
            sh(&bins.display().to_string()),
        );
        for (name, p) in &pkgs {
            if let Some(i) = &p.install {
                util::copy(&self.root.join(i), &dir.join(Path::new(i).file_name().unwrap()))?;
            }
            let fname = name.replace(['.'], "_");
            s += &format!("\npackage_{fname}() {{\n{}", Self::pkg_meta(p, false, "    ", &p.depends, &[], &[], true));
            s += &if p.is_meta() { "    true\n".to_string() } else { self.package_body(name, p)? };
            s += "}\n";
        }
        util::write(&dir.join("PKGBUILD"), &s)?;
        let dist = self.target_dir.join("syngui/dist");
        std::fs::create_dir_all(&dist)?;
        let conf = dir.join("makepkg.conf");
        util::write(&conf, format!("source /etc/makepkg.conf\nCARCH={}\nCHOST={}\nPKGDEST={}\n", target.arch(), target.triple(), sh(&dist.display().to_string())))?;
        util::step(format!("makepkg {} ({} пакетов, {})", self.spec.name, names.len(), target.arch()));
        let mut c = Command::new("makepkg");
        c.current_dir(&dir).arg("--config").arg(&conf).args(["-f", "--noconfirm", "--nodeps", "--noextract", "--noprepare"]).env("PKGDEST", &dist);
        util::run(c)?;
        let mut out = Vec::new();
        for n in &names {
            let f = dist.join(format!("{n}-{}-{}-{}.pkg.tar.zst", self.pkgver(), self.pkgrel(), target.arch()));
            if !f.is_file() {
                bail!("makepkg не оставил {}", f.display());
            }
            out.push(f);
        }
        Ok((out, dir.join("pkg")))
    }

    /// Архивы `<пакет>-<вер>-<арх>.tar.zst` (для -bin) из собранных пакетов; метапакеты пропускаются.
    pub fn tars(&self, only: &[String], target: Target, profile: Profile, strip: bool) -> Result<Vec<PathBuf>> {
        let (_, pkgdir) = self.arch(only, target, profile, strip)?;
        let dist = self.target_dir.join("syngui/dist");
        let mut out = Vec::new();
        for (name, p) in self.selected(only)? {
            if p.is_meta() {
                continue;
            }
            let f = dist.join(format!("{name}-{}-{}.tar.zst", self.version, target.arch()));
            let mut c = Command::new("tar");
            c.arg("--zstd").args(["--owner=0", "--group=0", "--numeric-owner"]);
            for x in [".PKGINFO", ".BUILDINFO", ".MTREE", ".INSTALL"] {
                c.arg(format!("--exclude=./{x}"));
            }
            c.arg("-cf").arg(&f).arg("-C").arg(pkgdir.join(name)).arg(".");
            util::run(c)?;
            util::sha256_file(&f)?;
            util::note(f.display().to_string());
            out.push(f);
        }
        Ok(out)
    }

    /// AUR: на каждый пакет `<имя>-git` и `<имя>-bin`; метапакеты — с зависимостями на `-git`/`-bin`.
    pub fn aur(&self, only: &[String], out: &Path) -> Result<Vec<PathBuf>> {
        let url = self.spec.url.clone().context("в syngui-release.toml нет url — нужен для AUR")?;
        let url = url.trim_end_matches('/').trim_end_matches(".git").to_string();
        let repo = url.rsplit('/').next().unwrap_or("src").to_string();
        let maint = format!("# Maintainer: {}\n# Сгенерировано syngui package -f aur --workspace из syngui-release.toml — правки делать там.\n", self.spec.maintainer.as_deref().unwrap_or("Alexeyev Vitaly"));
        let names: Vec<&str> = self.packages.iter().map(|(n, _)| n.as_str()).collect();
        let mut made = Vec::new();
        for (name, p) in self.selected(only)? {
            if p.aur == Some(false) {
                continue;
            }
            for suffix in ["git", "bin"] {
                let pkgname = format!("{name}-{suffix}");
                let mut s = maint.clone();
                s += &format!("pkgname={pkgname}\n");
                s += &format!("pkgver={}\npkgrel=1\narch=('x86_64' 'aarch64')\nurl={}\nlicense=({})\n", if suffix == "git" { format!("{}.r0", self.pkgver()) } else { self.version.clone() }, sh(&url), self.license());
                if p.is_meta() {
                    // зависимости на пакеты выпуска — их -git/-bin
                    let dep = |d: &String| if names.contains(&d.as_str()) { format!("{d}-{suffix}") } else { d.clone() };
                    let depends: Vec<String> = p.depends.iter().map(dep).collect();
                    let mut q = p.clone();
                    let fix = |v: &Vec<String>| v.iter().map(|o| match o.split_once(':') { Some((n, r)) if names.contains(&n.trim()) => format!("{}-{suffix}:{r}", n.trim()), _ => o.clone() }).collect::<Vec<_>>();
                    q.optdepends = fix(&p.optdepends);
                    q.optdepends_en = fix(&p.optdepends_en);
                    s += &Self::pkg_meta(&q, true, "", &depends, std::slice::from_ref(name), std::slice::from_ref(name), false);
                    s += "\npackage() {\n    true\n}\n";
                } else if suffix == "git" {
                    let mut mk: Vec<String> = self.spec.makedepends.clone();
                    mk.extend(p.makedepends.iter().cloned());
                    if !mk.iter().any(|m| m == "git") {
                        mk.push("git".into());
                    }
                    s += &Self::pkg_meta(p, true, "", &p.depends, std::slice::from_ref(name), &[name.clone(), format!("{name}-bin")], true);
                    let opts = if self.spec.options.is_empty() { vec!["!lto".to_string()] } else { self.spec.options.clone() };
                    s += &format!("makedepends=({})\noptions=({})\nsource=(\"{repo}::git+{url}.git\")\nsha256sums=('SKIP')\n", mk.iter().map(|x| sh(x)).collect::<Vec<_>>().join(" "), opts.iter().map(|x| sh(x)).collect::<Vec<_>>().join(" "));
                    let builds: Vec<String> = self.cargo_plans(&[&(name.clone(), p.clone())]).into_iter().map(|(_, a)| format!("    cargo build --offline --release {}\n", a.join(" "))).collect();
                    s += &format!(
                        r#"
pkgver() {{
    cd "$srcdir/{repo}"
    if git describe --tags >/dev/null 2>&1; then
        git describe --long --tags --abbrev=7 | sed 's/^v//;s/\([^-]*-g\)/r\1/;s/-/./g'
    else
        printf '%s.r%s.g%s' {ver} "$(git rev-list --count HEAD)" "$(git rev-parse --short=7 HEAD)"
    fi
}}

prepare() {{
    cd "$srcdir/{repo}"
    export RUSTUP_TOOLCHAIN=stable
    # Без --locked: git-зависимости (syngui и др.) свежие, Cargo.lock обновится.
    cargo fetch --target "$(rustc -vV | sed -n 's/host: //p')"
}}
"#,
                        ver = sh(&self.version)
                    );
                    if !builds.is_empty() {
                        s += &format!("\nbuild() {{\n    cd \"$srcdir/{repo}\"\n    export RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR=target\n{}}}\n", builds.concat());
                    }
                    // $srcdir задан только при запуске функций, не при чтении PKGBUILD
                    s += &format!("\npackage() {{\n    local _root=\"$srcdir/{repo}\" _bins=\"$srcdir/{repo}/target/release\"\n{}}}\n", self.package_body(name, p)?);
                } else {
                    let tar = |a: &str| format!("{name}-{}-{a}.tar.zst", self.version);
                    let sum = |a: &str| std::fs::read_to_string(self.target_dir.join("syngui/dist").join(format!("{}.sha256", tar(a)))).ok().and_then(|s| s.split_whitespace().next().map(str::to_string)).unwrap_or_else(|| "SKIP".into());
                    s += &Self::pkg_meta(p, true, "", &p.depends, std::slice::from_ref(name), &[name.clone(), format!("{name}-git")], true);
                    s += "options=('!strip' '!debug')\n";
                    s += &format!("noextract=({} {})\n", sh(&tar("x86_64")), sh(&tar("aarch64")));
                    for a in ["x86_64", "aarch64"] {
                        s += &format!("source_{a}=(\"{t}::{url}/releases/download/v$pkgver/{t}\")\nsha256sums_{a}=('{}')\n", sum(a), t = tar(a));
                    }
                    s += &format!("\npackage() {{\n    bsdtar -xpf \"$srcdir/{name}-$pkgver-$CARCH.tar.zst\" -C \"$pkgdir\"\n}}\n");
                }
                let d = out.join(&pkgname);
                util::write(&d.join("PKGBUILD"), &s)?;
                if let Some(i) = p.install.as_ref().filter(|_| !p.is_meta()) {
                    util::copy(&self.root.join(i), &d.join(Path::new(i).file_name().unwrap()))?;
                }
                let mut c = Command::new("makepkg");
                c.current_dir(&d).arg("--printsrcinfo");
                let info = util::capture(c).with_context(|| format!(".SRCINFO для {pkgname}"))?;
                util::write(&d.join(".SRCINFO"), info)?;
                made.push(d);
            }
        }
        util::note(format!("{} PKGBUILD в {}", made.len(), out.display()));
        Ok(made)
    }
}

/// `[workspace.package] version` (или `[package] version`) из корневого Cargo.toml.
fn workspace_version(root: &Path) -> Result<String> {
    let t: toml::Value = toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml"))?)?;
    t.get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .or_else(|| t.get("package").and_then(|p| p.get("version")))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .context("версия: нет [workspace.package] version — задайте version в syngui-release.toml")
}

/// Поставить пакеты одной транзакцией (sudo pacman -U).
pub fn install(pkgs: &[PathBuf]) -> Result<()> {
    util::step(format!("pacman -U ({} пакетов)", pkgs.len()));
    let mut c = Command::new("sudo");
    // пароль из конвейера (`echo … | syngui …`) — sudo -S
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        c.arg("-S");
    }
    c.args(["pacman", "-U", "--noconfirm"]).args(pkgs);
    util::run(c)
}
