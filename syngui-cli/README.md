# syngui — сборка, упаковка и запуск приложений на syngui

Одна команда вместо `build-apk.sh`, `build-ipa.sh`, ручных PKGBUILD и кросс-окружений:
Linux x86_64 и aarch64 (компьютеры и телефоны), Windows x86_64, Android APK, iOS IPA, веб (WebAssembly,
в том числе Telegram Mini App), пакеты Arch и AUR —
для одного приложения и для выпуска всего рабочего пространства из многих пакетов (synshell).

```sh
cargo install --path syngui-cli --profile fast-release   # бинарник `syngui`

syngui doctor                          # что есть для сборки под каждую цель
syngui info -p syncook                 # что утилита знает о приложении
syngui build -t android                # APK (fast-release; подпись релизным ключом, если он есть)
syngui build -t linux-aarch64,ios      # несколько целей
syngui run -d android                  # собрать под ABI устройства, поставить, запустить, logcat
syngui run -d ios                      # .ipa → plumesign (Apple ID) → iPhone
syngui run -d phone                    # aarch64 → телефон (synport) в /usr/local и запуск в его графической сессии
syngui run -d phone --pkg              # то же пакетом pacman (sudo pacman -U, сверка версии и даты сборки)
syngui run -d ssh:xiaomi-12t-pro --no-install -- --arg   # только запустить установленную (хост из ~/.ssh/config)
syngui package -w -f arch -t linux-aarch64 --install -d phone   # пакеты выпуска — на устройство
syngui package -f apk --release        # релиз: LTO, только релизным ключом → dist + .sha256
syngui package -f tar -t all-linux --release   # архивы релиза x86_64 и aarch64 (их скачивает -bin)
syngui package -f arch -t all-linux    # пакеты pacman обеих архитектур
syngui package -f aur                  # PKGBUILD + .SRCINFO для <имя>-git и <имя>-bin
syngui package -f ipa                  # неподписанный .ipa
syngui package -f zip                  # Windows: <bin>-<вер>-windows-x86_64.zip (exe + docs)
syngui build -t windows-x86_64         # exe через mingw-w64
syngui build -t web                    # WebAssembly → target/syngui/web/<крейт>/ (index.html, pkg/, ассеты)
syngui run -d web                      # собрать и раздать по http://127.0.0.1:8080/ (--listen 0.0.0.0:8000)
syngui package -f tar --portable       # linux-x86_64 в контейнере с glibc 2.31 (Ubuntu 20.04+, Debian 11+)
syngui package -w -f arch --install        # весь выпуск рабочего пространства, pacman -U одной транзакцией
echo ПАРОЛЬ | syngui package -w -f arch --install   # без терминала (агент, IDE): пароль sudo из stdin или $SYNGUI_SUDO_PASSWORD
syngui release 0.2.0                   # выпуск: сухой прогон — что будет сделано
syngui release 0.2.0 --apply           # версия в Cargo.toml, коммит «Выпуск 0.2.0», тег v0.2.0 (локально)
syngui release 0.2.0 --publish         # + архивы, APK/IPA, git push, gh release create, AUR с суммами
syngui keygen -p syncook               # релизный ключ Android в ~/.secrets/syncook-android
syngui init                            # заготовка [package.metadata.syngui] в Cargo.toml
```

Приложение выбирается `-p <крейт>`, иначе — крейт текущего каталога, иначе — единственный крейт
рабочего пространства с `[package.metadata.syngui]`.
Для `package -f arch|tar|aur`: если крейт входит в пакет `syngui-release.toml` рабочего пространства
(пакет с тем же именем или пакет с его программами), сборка идёт по описанию выпуска — как
`-w --only <пакет>` (cross, files, depends и т. д.), а не голым крейтом.

Профили: по умолчанию `fast-release` (если в Cargo.toml его нет — задаётся через `--config`),
`--release` — LTO для публикации, `--debug` — dev. `package` тоже по умолчанию собирает fast-release
(`--fast` принимается для совместимости); `package --release` — LTO для публикации (APK — релизным ключом).

Свои фреймворки всегда последние: перед `build`, `package`, `run` (кроме `--no-build`) и `release --apply|--publish`
утилита делает `cargo update` для git-зависимостей с `github.com/VitaminDB/` (syngui, synaptix…) —
`Cargo.lock` сам закреплённый коммит не двигает. Отключить: `SYNGUI_NO_UPDATE=1`; нет сети — сборка с прежними коммитами.

После каждой упаковки в `target/syngui/dist/` обновляется общий `SHA256SUMS`.

Результаты: служебное — `target/syngui/<вид>/<крейт>/` (сгенерированный gradle-проект, .app, stage),
готовое к выкладке — `target/syngui/dist/` (APK, IPA, `*.tar.zst`, `*.pkg.tar.zst` и `.sha256`).

## Метаданные приложения

Все поля необязательны: без них имя — имя крейта, значок — `data/icons/<bin>.svg`,
ярлык — `data/<bin>.desktop`, id — из прежнего `[package.metadata.android] package`
или `io.github.vitamindb.<крейт>`.

```toml
[package.metadata.syngui]
name = "Мамины рецепты"            # имя под значком (Android, iOS)
id = "kz.syn.syncook"              # id приложения / bundle id
icon = "assets/icon.svg"           # SVG значка (или PNG: Android/iOS масштабируют, Linux — hicolor/WxH)
bin = "syncook"                    # программа, если их несколько
desktop = "data/syncook.desktop"   # ярлык Linux
description_en = "Family recipes"  # описание для AUR (иначе package.description)
languages = ["ru", "en"]           # iOS: первый — основной
docs = ["README.md", "LICENSE"]    # в архивы (рядом с install.sh, в zip) и пакеты (usr/share/doc|licenses/<пакет>)
icons_png = "assets/icon/app-{size}.png"  # PNG 16–512 по шаблону; без него рисуются из SVG

[package.metadata.syngui.android]
id = "kz.syn.syncook"              # если отличается от общего id
lib = "syncook"                    # android.app.lib_name (по умолчанию [lib] name)
features = ["android"]             # возможности cargo (по умолчанию ["android"], без default)
default_features = false
abis = ["arm64-v8a"]               # arm64-v8a, armeabi-v7a, x86_64
min_sdk = 30
target_sdk = 35
version_code = 3                   # по умолчанию из версии: 1.2.3 → 1002003
version_code_offset = 20000000     # прибавка к коду из версии (переход со своей схемы номеров)
cleartext_traffic = true           # usesCleartextTraffic: http:// и ws:// (по умолчанию нет)
allow_backup = false               # android:allowBackup (по умолчанию true)
permissions = ["INTERNET"]         # короткие или полные имена
strip = false                      # оставить символы в .so (по умолчанию strip, кроме --debug)
ffmpeg = true                      # статический FFmpeg из syngui/android-deps/ffmpeg/<abi>
foreground_service = "quit_smoking_timer"  # SynGuiForegroundService (specialUse) + разрешения
background = "#FFF8F0"             # фон окна до первого кадра
orientation = "portrait"
icon_foreground = "assets/icon/icon-foreground.svg"  # адаптивный значок (Android 8+): холст 108dp,
                                   # рисунок в центральных ~61 %, фон прозрачный; он же — монохромный слой
icon_background = "#F6EFE3"        # фон адаптивного значка: цвет или SVG
icon_monochrome = "assets/icon/mono.svg"  # свой слой тематических значков Android 13 (необязательно)
fullscreen = true                  # тема без строки состояния с первого кадра (windowFullscreen);
                                   # панель навигации прячет App::fullscreen(true) / syngui::set_fullscreen
manifest_application = "android/application.xml"  # XML внутрь <application> (файл или строка с «<»)
manifest = "<uses-feature android:name=\"android.hardware.nfc\" />"
res = "android/res"                # свои ресурсы поверх сгенерированных
java = "android/java"              # свои Java-классы
gradle_dependencies = ["androidx.work:work-runtime:2.9.1"]
keystore = "~/.secrets/syncook-android"   # по умолчанию ~/.secrets/<крейт>-android
key_alias = "syncook"              # по умолчанию имя крейта
gradle_project = "android"         # своё gradle-приложение вместо сгенерированного

[package.metadata.syngui.ios]
bundle_id = "io.github.vitamindb.syncycle"
min_ios = "14.0"
icon_viewbox = "8 8 112 112"       # обрезать поля значка (iOS скругляет сам)
icon_unround = 'rx="30"'           # убрать скругление фона
icon = "data/icons/ios.svg"        # или свой квадратный значок
status_bar_hidden = true
strip = false                      # оставить символы (по умолчанию strip, кроме --debug)
features = ["ios"]
default_features = false

[package.metadata.syngui.linux]
package = "syncook"                # имя пакета Arch/AUR (по умолчанию — программа)
bin_name = "syncook"               # имя программы в usr/bin, если отличается от [[bin]] (ardor_mouse → ardor-mouse)
install = "packaging/syncook.install"   # скрипт pacman .install (post_install…): пакет arch, AUR -git и -bin
depends = ["gcc-libs", "fontconfig", "libxkbcommon", "wayland", "vulkan-icd-loader"]
optdepends = ["ffmpeg: видео"]
makedepends = ["nasm"]             # для -git, к cargo git clang pkgconf
features = ["desktop"]
default_features = true
aarch64 = { features = ["npu"], default_features = false }   # своё для архитектуры (и x86_64 = {…})
files = ["data/sounds:usr/share/syncook/sounds"]   # доп. файлы пакета «откуда:куда»
installer = "packaging/linux/install.sh"   # свой установщик в tar (иначе генерируется: ~/.local или /usr)
portable_image = "docker.io/library/rust:1-bullseye"   # образ для --portable
portable_packages = ["pkg-config"]  # apt-пакеты в образе
epoch = 1                          # epoch в PKGBUILD -git, -bin и arch (provides — без него)
arches = ["x86_64"]                # архитектуры выпуска: -bin/-git и tar в release --publish (иначе — из release.artifacts, иначе обе)
git_sources = ["syngui::git+https://github.com/VitaminDB/syngui.git"]   # соседи для -git (path-зависимости ../syngui), SKIP
build_env = ["PATH=/opt/cuda/bin:$PATH"]   # export в build() -git (значение раскрывает bash)

[package.metadata.syngui.windows]
features = ["gui"]
default_features = true
packages = ["update_helper"]       # программы других крейтов рабочего пространства — в zip рядом
files = ["assets/app.ico"]         # дополнительные файлы в zip

[package.metadata.syngui.web]       # syngui build -t web / run -d web
features = ["web"]                 # возможности cargo для wasm32 (по умолчанию — крейта по умолчанию)
default_features = false           # по умолчанию true
assets = "web/assets"              # содержимое — в корень сайта (по умолчанию web/assets, если есть)
index = "web/index.html"           # своя страница (по умолчанию web/index.html, если есть; иначе генерируется)
title = "Пульт"                    # <title> сгенерированной страницы (по умолчанию name)
background = "#17212b"             # фон страницы до первого кадра (по умолчанию #000)
telegram = true                    # Telegram Mini App: telegram-web-app.js до модуля приложения
gzip = true                        # pkg/*.wasm.gz и pkg/*.js.gz (gzip -9) при --release и в пакетах выпуска (по умолчанию true)

[package.metadata.syngui.release]          # syngui release --publish
artifacts = ["tar-portable", "zip"]        # tar, tar-x86_64, tar-aarch64, tar-portable, zip, apk, ipa
assets = ["~/путь/к/готовым/файлам/*"]     # внешние готовые файлы в релиз GitHub
```

Ярлык ставится под своим именем, если задан `desktop` (`kz.syn.synport.desktop` — id AppStream);
значки называются по `Icon=` из ярлыка.

## Архив tar (Linux)

`<bin>-<вер>-<арх>.tar.zst`: дерево `usr/` (его ставят `-bin` и пакет Arch), документы и `install.sh`
(`./install.sh` — в `~/.local`, `sudo ./install.sh /usr` — в систему). `--portable` (linux-x86_64)
собирает в podman/docker с образом rust:1-bullseye: рабочее пространство монтируется по тому же пути,
path-зависимости вне его (`../syngui` через `[patch]`) — только для чтения, кэш cargo —
`~/.cache/syngui/portable`. Утилита печатает, какая glibc нужна бинарнику.

## Windows

`-t windows-x86_64`: `x86_64-pc-windows-gnu`, линковка `x86_64-w64-mingw32-gcc`, strip mingw.
Ресурсы exe (значок, VERSIONINFO, манифест) — build.rs крейта (winresource + windres).
`[package.metadata.syngui.windows] features` — возможности для этой цели.

## Веб (WebAssembly)

`-t web` (`wasm`): `cargo build --lib --target wasm32-unknown-unknown` (профиль как у остальных целей:
fast-release, `--release` — LTO и `wasm-opt -Oz`, если стоит binaryen; `--debug` — dev и `wasm-bindgen --debug`),
затем `wasm-bindgen --target web --no-typescript`. При `--release` и в пакетах выпуска рядом кладутся `pkg/*.wasm.gz` и
`pkg/*.js.gz` (gzip -9; `web.gzip = false` — не класть) — сервер отдаёт их с `Content-Encoding: gzip`, если браузер
принимает gzip (wasm 6,3 → 2,4 МБ). Крейту нужен `[lib] crate-type = ["cdylib", "rlib"]` и
точка входа `#[wasm_bindgen(start)]` (пример — `examples/widget_gallery_mss/src/lib.rs`, `run_app`).
Результат — готовый к выкладке каталог `target/syngui/web/<крейт>/`: `index.html`, `pkg/<lib>.js`,
`pkg/<lib>_bg.wasm` и содержимое каталога ассетов (`web/assets/fonts/X.ttf` → `fonts/X.ttf` — путь для
`.with_font_url("fonts/X.ttf")`).

Страница: `web/index.html` крейта (или `web.index`), иначе генерируется — полноэкранный canvas,
viewport с `interactive-widget=resizes-content` (экранная клавиатура сжимает canvas), импорт
`pkg/<lib>.js` и `init()`. С `telegram = true` в `<head>` добавляется
`<script src="https://telegram.org/js/telegram-web-app.js">` (и в свою страницу, если его там нет);
API — `syngui::web::telegram`.

wasm-bindgen-cli должен быть той же версии, что крейт `wasm-bindgen` в Cargo.lock: если в PATH другая,
утилита один раз ставит нужную в `~/.cache/syngui/wasm-bindgen-<версия>/` (`cargo install --root`) и
берёт её; `syngui doctor -t web` показывает обе версии.

`run -d web` собирает (`--no-build` — нет) и раздаёт каталог встроенным HTTP-сервером (GET/HEAD,
`application/wasm`, без кэша) и `<файл>.gz` с `Content-Encoding: gzip` при `Accept-Encoding: gzip`; адрес — `--listen` (по умолчанию `127.0.0.1:8080`, занят — следующий
порт). Для Telegram нужен HTTPS — выложите каталог на свой сервер или пробросьте туннелем.
`--features`/`--no-default-features` у `run` заменяют возможности из метаданных.

## Выпуск рабочего пространства (`--workspace`)

Для проектов из многих пакетов (synshell): описание — `syngui-release.toml` в корне рабочего
пространства (или путь в `[workspace.metadata.syngui] release`). Из него генерируются bash-функции
`package_<имя>()`; одни и те же функции работают в локальном split-PKGBUILD, в AUR `<имя>-git` и при
упаковке архивов для `<имя>-bin`.

```sh
syngui package -w -f arch --install                 # собрать всё (fast-release), makepkg, pacman -U одной транзакцией
syngui build -w -t linux-aarch64                    # только собрать программы выпуска (без упаковки) → target/syngui/release/<имя>/<арх>/bin
syngui package -w -f arch -t linux-aarch64          # пакеты для телефона (sysroot)
syngui package -w -f tar -t all-linux               # архивы <пакет>-<вер>-<арх>.tar.zst для -bin
syngui package -w -f aur                            # <пакет>-git и <пакет>-bin на каждый пакет
syngui package -w -f arch --only syn-vm,synshell-core   # часть выпуска
```

```toml
name = "synshell"                      # pkgbase
url = "https://github.com/VitaminDB/synshell"
license = ["GPL-3.0-only"]
makedepends = ["cargo", "git", "clang", "pkgconf"]
options = ["!lto"]                     # для -git
arches = ["x86_64"]                    # архитектуры выпуска: архивы релиза и arch=() в AUR (по умолчанию обе)
# version — из [workspace.package]; pkgrel — число коммитов (или pkgrel = N)

[crates.synaptixd]                     # особая сборка крейта (отдельный cargo build)
features = ["cuda", "npu"]
features_env = "SYNAPTIXD_FEATURES"    # SYNAPTIXD_FEATURES=cuda — переопределить
no_default_features = false
[crates.synaptixd.aarch64]             # своё для архитектуры (в AUR -git — выбор по $CARCH)
features = ["npu"]

[packages.synshell]                    # метапакет: без файлов
description = "…"
description_en = "…"                   # для AUR
depends = ["synshell-core", "syn-vm"]  # в AUR — synshell-core-git / -bin
optdepends = ["synshell-ai: ассистент"]
optdepends_en = ["synshell-ai: assistant"]

[packages.synshell-core]
description = "…"
bins = ["synwm", "synfsd"]             # программы (bin, имя может отличаться от крейта) → /usr/bin
apps = ["synsettings"]                 # программа + ярлык и значок из data/ её крейта
desktop = ["synshot"]                  # только ярлык и значок
files = [
  { from = "data/synshell-session", to = "usr/bin/", mode = "755" },       # «…/» — в каталог
  { from = "data/sounds/themes/*", to = "usr/share/sounds/" },             # маска; каталоги — целиком
  { from = "crates/x/data/OFL.txt", to = "usr/share/licenses/{pkgname}/fonts-OFL.txt" },
  { run = "echo '[Default Applications]'; cat crates/*/data/defaults.mimeapps | grep '='", to = "usr/share/applications/synshell-mimeapps.list" },
]
cross = [                              # программы под другую цель (гостевые агенты и т. п.)
  { bin = "syn-vm-agent", target = "x86_64-unknown-linux-musl", to = "usr/lib/syn-vm/agent/" },
  # tier 3 (нет std у rustup): nightly + -Zbuild-std, нужен rust-src; win7 — Windows 7…11 одним exe
  { bin = "syn-vm-agent", target = "x86_64-win7-windows-gnu", to = "usr/lib/syn-vm/agent/", build_std = true },
]
webs = [                               # веб-сборки крейтов (как syngui build -t web; по умолчанию fast-release, с --release — release)
  { crate = "syn-courier-panel", to = "usr/share/syncourier/panel" },   # index.html, pkg/ (+ .gz), ассеты
]
links = { "usr/bin/syndesktop" = "synwm" }
dirs = ["usr/share/synaptix/models"]   # пустые каталоги
provides = ["syndesktop"]
conflicts = ["syndesktop"]
replaces = ["syndesktop"]
backup = ["etc/synaptix/synaptixd.toml"]
groups = ["synshell-apps"]
install = "packaging/synshell-core.install"
aur = false                            # не публиковать в AUR
aur_source = true                      # ещё и пакет <имя> без суффикса: сборка из исходников тега
                                       # (архив GitHub v<вер>.tar.gz, сумма — при release --publish)
```

Программы копируются из target/ и очищаются от символов (`--no-strip` — оставить). Веб-сборки (`webs`) собираются как
`syngui build -t web` (метаданные `[package.metadata.syngui.web]` крейта, gzip всегда по `web.gzip`), каталог
`target/syngui/web/<крейт>/` копируется в пакет по пути `to`. В AUR (`-git`, сборка из тега) syngui нет — build()
повторяет то же командами: цель wasm32-unknown-unknown у rustup (makedepends — rustup вместо rust/cargo, плюс
binaryen для wasm-opt), в prepare() — `cargo install wasm-bindgen-cli` той версии, что крейт wasm-bindgen в
Cargo.lock, в `$srcdir/wasm-bindgen`; затем wasm-bindgen, wasm-opt, gzip -9, ассеты и index.html (сгенерированная
страница встраивается в PKGBUILD). Пакеты
кладутся в `target/syngui/dist`; `--install` ставит их `sudo pacman -U` (пароль можно подать в stdin).

## Android

Gradle-проект генерируется из метаданных в `target/syngui/android/<крейт>` при каждой сборке
(неизменённые файлы не перезаписываются): GameActivity (версия согласована с android-activity),
FileProvider `<id>.syngui.files` для `syngui::platform`, тема без заголовка с вырезом под камеру,
значки mipmap из SVG. Каталог `android/` в репозитории приложения больше не нужен.

Подпись — после gradle: `zipalign -P 16` (страницы 16 КБ) и `apksigner`. Ключ —
`~/.secrets/<крейт>-android/{release.jks,password}` (или `SYNGUI_ANDROID_KEYSTORE`), никогда не в
репозитории. Если релизный ключ есть, им подписываются и сборки себе — они ставятся поверх релиза;
нет — отладочный `~/.android/debug.keystore`. `package -f apk` без релизного ключа не собирается.

## Linux aarch64

На x86_64 — кросс-сборка clang + lld с glibc и библиотеками телефона из sysroot:
`SYNGUI_AARCH64_SYSROOT`, `~/.local/share/syngui/sysroot-aarch64` или rootfs устройства
synmobile (`devices/*/build/rootfs`, сначала xiaomi-diting). FFmpeg берётся из sysroot.

## iOS

Без Xcode: `aarch64-apple-ios`, iOS SDK из `IOS_SDK` (по умолчанию
`~/.local/share/ios-sdk/src18/iPhoneOS18.6.sdk`, не ниже 18), clang + ld64.lld.
.ipa не подписан — `run -d ios` подписывает его на Apple ID и ставит через `plumesign`
(`SYNGUI_IOS_UDID` — выбрать устройство).

## Выпуск (`syngui release`)

Без флагов — сухой прогон: версия было → стало, какие артефакты соберутся (APK — только при релизном
ключе), куда пойдут push и релиз GitHub. `--apply` поднимает версию (`[workspace.package]` корня или
`[package]` крейта), обновляет Cargo.lock, коммитит «Выпуск X» и ставит тег `vX` — без публикации.
`--publish` (после `--apply` или сразу): архивы `-t all-linux --release` (`-w` при `syngui-release.toml`),
APK/IPA крейтов с разделами android/ios в метаданных (`--no-mobile` — без них), всё в
`target/syngui/releases/vX` с SHA256SUMS, `git push origin HEAD vX`, `gh release create`, затем AUR
`<пакет>-git/-bin` с суммами релиза в `target/syngui/aur` — их выкладку в AUR делает человек.
Нужна чистая рабочая копия; существующий тег на другом коммите — ошибка.

Состав релиза приложения задаёт `[package.metadata.syngui.release] artifacts` (без него — tar обеих
архитектур и APK/IPA по разделам android/ios). Готовые внешние файлы (образы, исходники GPL) —
`assets` (маски, `~`) или `--assets <каталог|файл>` (можно несколько; у выпуска рабочего пространства —
`assets` в syngui-release.toml): файлы прикладываются как есть, без копирования, подкаталоги — архивом
`<имя>.tar`; чужой SHA256SUMS заменяется общим — в нём суммы всех файлов релиза. Файлы от 2 ГиБ
GitHub не принимает — это проверяется до загрузки.

## Устройства с Linux (`-d phone`, `-d ssh:<хост>`)

`phone` — связь через synport разработчика (synmobile/tools/synport: USB-сеть или synlink, root),
`ssh:<хост>` — обычный ssh (хост из `~/.ssh/config`). `run` собирает aarch64, ставит и запускает:
окружение (WAYLAND_DISPLAY, VK_ICD_FILENAMES, MESA_LOADER_DRIVER_OVERRIDE…) берётся у клиента
графической сессии (synmobile/synpanel/syndesktop), запуск — от его пользователя через setsid;
журнал — `/tmp/syngui-run-<bin>.log` на устройстве. Без этого по голому ssh программа рисовала бы
программно (llvmpipe). По умолчанию — в `/usr/local`; если программа уже стоит пакетом pacman,
утилита откажется (вторая копия перекрыла бы её) и предложит `--pkg`: пакет `-f arch`,
`pacman -U` на устройстве и сверка версии и даты сборки. Не под root — sudo
(`SYNGUI_DEVICE_SUDO_PASSWORD` — пароль). Флаги `run`: `--features`, `--no-default-features`,
`--env KEY=VAL`, `--no-install`, `--no-launch`, аргументы программы — после `--`.

## Переменные

| Переменная | Что |
|---|---|
| `SYNGUI_VERBOSE=1` | показывать запускаемые команды |
| `SYNGUI_DIR` | исходники syngui (android-deps, Java) — по умолчанию репозиторий, из которого собрана утилита |
| `ANDROID_HOME`, `ANDROID_NDK_HOME`, `JAVA_HOME` | SDK, NDK (иначе /opt/android-ndk, $ANDROID_HOME/ndk/*), JDK 17–21 |
| `ANDROID_SERIAL` | устройство для `run -d android` |
| `SYNGUI_ANDROID_KEYSTORE` | каталог ключа подписи вместо ~/.secrets/<крейт>-android |
| `SYNGUI_AARCH64_SYSROOT` | sysroot для linux-aarch64 |
| `IOS_SDK`, `SYNGUI_IOS_UDID` | iOS SDK, iPhone для установки |
| `SYNPORT` | путь к synport разработчика (с ssh/scp) для `-d phone` |
| `SYNGUI_DEVICE_SUDO_PASSWORD` | пароль sudo на устройстве (`-d ssh:` не под root) |
| `<CRATE>_FEATURES` (`features_env`) | возможности особого крейта выпуска |
