//! Устройства с Linux (телефон с synshell, любой хост по ssh): копирование, команды, установка
//! пакетов pacman и запуск программы в графической сессии устройства.

use crate::util::{self, sh};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// `phone` (через synport: USB-сеть или synlink, root) или `ssh:<хост>` (обычный ssh, хост из ~/.ssh/config).
pub struct Device {
    name: String,
    synport: Option<(PathBuf, PathBuf)>,
    host: Option<String>,
}

/// synport разработчика (с ssh/scp): $SYNPORT, PATH или synmobile/tools/synport (у публичного synport их нет).
fn synport() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SYNPORT") {
        return Some(PathBuf::from(p));
    }
    util::which("synport").or_else(|| {
        let h = util::home();
        [
            h.join("Storage/Проекты/2027/synmobile/tools/synport/target/fast-release/synport"),
            h.join("Projects/2027/synmobile/tools/synport/target/fast-release/synport"),
        ]
        .into_iter()
        .find(|p| p.is_file())
    })
}

impl Device {
    pub fn parse(d: &str) -> Result<Device> {
        if d == "phone" {
            let sp = synport().context("нет synport (SYNPORT=<путь>) — им идёт связь с телефоном; или --device ssh:<хост>")?;
            // synport ищет файлы устройств от корня synmobile
            let h = util::home();
            let root = [h.join("Storage/Проекты/2027/synmobile"), h.join("Projects/2027/synmobile")]
                .into_iter()
                .find(|p| p.is_dir())
                .unwrap_or_else(|| PathBuf::from("."));
            Ok(Device { name: d.into(), synport: Some((sp, root)), host: None })
        } else if let Some(h) = d.strip_prefix("ssh:") {
            Ok(Device { name: d.into(), synport: None, host: Some(h.into()) })
        } else {
            bail!("устройство {d}: phone или ssh:<хост>")
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn copy(&self, local: &Path, remote: &str) -> Result<()> {
        let c = match (&self.synport, &self.host) {
            (Some((sp, root)), _) => {
                let mut c = Command::new(sp);
                c.current_dir(root).arg("scp").arg(local).arg(format!("phone:{remote}"));
                c
            }
            (_, Some(h)) => {
                let mut c = Command::new("scp");
                c.arg("-q").arg(local).arg(format!("{h}:{remote}"));
                c
            }
            _ => unreachable!(),
        };
        util::run(c)
    }

    fn cmd(&self, script: &str) -> Command {
        match (&self.synport, &self.host) {
            (Some((sp, root)), _) => {
                let mut c = Command::new(sp);
                c.current_dir(root).arg("ssh").arg(script);
                c
            }
            (_, Some(h)) => {
                let mut c = Command::new("ssh");
                c.arg(h).arg(script);
                c
            }
            _ => unreachable!(),
        }
    }

    pub fn exec(&self, script: &str) -> Result<()> {
        util::run(self.cmd(script))
    }

    pub fn capture(&self, script: &str) -> Result<String> {
        util::capture(self.cmd(script))
    }
}

/// Префикс для команд root: пусто под root, иначе sudo (пароль — из $SYNGUI_DEVICE_SUDO_PASSWORD, если задан).
const AS_ROOT: &str = r#"if [ "$(id -u)" = 0 ]; then R=""; else R="sudo"; if [ -n "${SUDO_PW:-}" ]; then printf '%s\n' "$SUDO_PW" | sudo -S -v 2>/dev/null; fi; fi"#;

fn sudo_env() -> String {
    match std::env::var("SYNGUI_DEVICE_SUDO_PASSWORD") {
        Ok(p) => format!("SUDO_PW={}; export SUDO_PW; ", sh(&p)),
        Err(_) => String::new(),
    }
}

/// Поставить пакеты pacman на устройство одной транзакцией и сверить дату сборки.
pub fn install_packages(dev: &Device, pkgs: &[PathBuf]) -> Result<()> {
    let tag = format!("syngui-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs());
    let mut remote = Vec::new();
    for p in pkgs {
        let r = format!("/tmp/{tag}-{}", p.file_name().unwrap().to_string_lossy());
        util::step(format!("{} → {}:{r}", p.display(), dev.name()));
        dev.copy(p, &r)?;
        remote.push(r);
    }
    let list = remote.iter().map(|r| sh(r)).collect::<Vec<_>>().join(" ");
    let script = format!("{}{AS_ROOT}; set -e; $R pacman -U --noconfirm {list}; rm -f {list}", sudo_env());
    util::step(format!("pacman -U на {} ({} пакетов)", dev.name(), pkgs.len()));
    dev.exec(&script)?;
    // сверка: установлена именно эта сборка (pkgver и builddate из .PKGINFO)
    for p in pkgs {
        let mut c = Command::new("bsdtar");
        c.arg("-xOf").arg(p).arg(".PKGINFO");
        let info = util::capture(c)?;
        let field = |k: &str| info.lines().find_map(|l| l.strip_prefix(&format!("{k} = ")).map(str::to_string)).unwrap_or_default();
        let (name, ver, date) = (field("pkgname"), field("pkgver"), field("builddate"));
        let got = dev.capture(&format!("pacman -Qi {} | sed -n 's/^\\(Version\\|Версия\\) *: //p'; date -d \"$(pacman -Qi {} | sed -n 's/^\\(Build Date\\|Дата сборки\\) *: //p')\" +%s 2>/dev/null || true", sh(&name), sh(&name)))?;
        let mut it = got.lines();
        let (gv, gd) = (it.next().unwrap_or("").trim().to_string(), it.next().unwrap_or("").trim().to_string());
        if gv != ver {
            bail!("{name}: на устройстве {gv}, ожидалось {ver}");
        }
        if !gd.is_empty() && gd != date {
            util::warn(format!("{name}: дата сборки на устройстве {gd}, у пакета {date}"));
        } else {
            util::note(format!("{name} {ver} — установлен"));
        }
    }
    Ok(())
}

/// Поставить дерево usr/ программы в /usr/local устройства (без pacman).
pub fn install_tree(dev: &Device, bin: &str, tarball: &Path) -> Result<()> {
    // если программа стоит пакетом pacman — вторая копия в /usr/local перекрыла бы её в PATH
    let owner = dev.capture(&format!("pacman -Qqo /usr/bin/{bin} 2>/dev/null || true"))?;
    if !owner.trim().is_empty() {
        bail!(
            "на {} {bin} стоит пакетом pacman ({}) — копия в /usr/local перекрыла бы его. Поставьте пакетом: syngui run -d {} --pkg (или syngui package [-w] -f arch -t linux-aarch64 --install -d {})",
            dev.name(),
            owner.trim(),
            dev.name(),
            dev.name()
        );
    }
    let remote_tar = format!("/tmp/{bin}-syngui-{}.tar", std::process::id());
    dev.copy(tarball, &remote_tar)?;
    let script = format!(
        "{}{AS_ROOT}; set -e; d=$(mktemp -d); tar -xf {remote_tar} -C $d; $R install -Dm755 $d/bin/{bin} /usr/local/bin/{bin}.new && $R mv -f /usr/local/bin/{bin}.new /usr/local/bin/{bin}; rm -f $d/bin/{bin}; $R cp -r $d/. /usr/local/; rm -rf $d {remote_tar}; echo установлено: /usr/local/bin/{bin}",
        sudo_env()
    );
    util::step(format!("установка в /usr/local на {}", dev.name()));
    dev.exec(&script)
}

/// Запустить программу в графической сессии устройства: окружение (WAYLAND_DISPLAY, VK_ICD_FILENAMES,
/// MESA_LOADER_DRIVER_OVERRIDE…) берётся у клиента сессии, запуск — от его пользователя, через setsid.
/// Без этого по голому ssh программа рисовала бы программно (llvmpipe).
pub fn launch(dev: &Device, bin: &str, args: &[String], env: &[String]) -> Result<()> {
    let argv = args.iter().map(|a| sh(a)).collect::<Vec<_>>().join(" ");
    let extra = env.iter().map(|kv| format!("export {};", sh(kv))).collect::<String>();
    let comm: String = bin.chars().take(15).collect();
    let script = format!(
        r#"set -e
B={b}
for p in $(pgrep -x {comm} || true); do kill $p 2>/dev/null || true; done
sleep 1
P=""
for c in $(pgrep -f 'synmobile|synpanel|synlauncher|syndesktop|syngame-shell' || true); do
    if tr '\0' '\n' < /proc/$c/environ 2>/dev/null | grep -q '^WAYLAND_DISPLAY='; then
        P=$c
        tr '\0' '\n' < /proc/$c/environ | grep -q '^VK_ICD_FILENAMES=' && break
    fi
done
[ -n "$P" ] || {{ echo "на устройстве нет клиента графической сессии (synmobile/syndesktop) — сессия не запущена?" >&2; exit 1; }}
F=/tmp/syngui-run-$B.env
tr '\0' '\n' < /proc/$P/environ | grep -E '^[A-Za-z_][A-Za-z0-9_]*=' | sed "s/'/'\\\\''/g; s/^\([^=]*\)=\(.*\)\$/export \1='\2'/" > $F
chmod 644 $F
U=$(stat -c %u /proc/$P); G=$(stat -c %g /proc/$P)
L=/tmp/syngui-run-$B.log
rm -f $L
CMD=". $F; {extra} cd \"\$HOME\"; setsid $B {argv} >$L 2>&1 </dev/null &"
if [ "$(id -u)" = "$U" ]; then sh -c "$CMD"; else setpriv --reuid=$U --regid=$G --init-groups sh -c "$CMD"; fi
sleep 3
if pgrep -x {comm} >/dev/null; then
    echo "запущено: $(pgrep -a -x {comm} | head -1) (пользователь $(stat -c %U /proc/$P), журнал $L)"
    grep -iE 'llvmpipe|panicked|error' $L | head -5 || true
else
    echo "программа завершилась сразу — журнал $L:" >&2; tail -20 $L >&2; exit 1
fi"#,
        b = sh(bin),
        extra = extra.replace('"', "\\\""),
        argv = argv.replace('"', "\\\""),
    );
    util::step(format!("запуск {bin} на {} в графической сессии", dev.name()));
    dev.exec(&script)
}
