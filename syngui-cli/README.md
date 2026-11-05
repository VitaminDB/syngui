# syngui — сборка, упаковка и запуск приложений на syngui

Одна команда вместо `build-apk.sh`, `build-ipa.sh`, ручных PKGBUILD и кросс-окружений:
Linux x86_64 и aarch64 (компьютеры и телефоны), Android APK, iOS IPA, пакеты Arch и AUR.

```sh
cargo install --path syngui-cli --profile fast-release   # бинарник `syngui`

syngui doctor                          # что есть для сборки под каждую цель
syngui info -p syncook                 # что утилита знает о приложении
syngui build -t android                # APK (fast-release; подпись релизным ключом, если он есть)
syngui build -t linux-aarch64,ios      # несколько целей
syngui run -d android                  # собрать под ABI устройства, поставить, запустить, logcat
syngui run -d ios                      # .ipa → plumesign (Apple ID) → iPhone
syngui run -d phone                    # aarch64 → телефон с synshell (synport) в /usr/local
syngui run -d ssh:root@192.168.8.45    # то же по обычному ssh
syngui package -f apk                  # релиз: --release (LTO), только релизным ключом → dist + .sha256
syngui package -f tar -t all-linux     # архивы релиза x86_64 и aarch64 (их скачивает -bin)
syngui package -f arch -t all-linux    # пакеты pacman обеих архитектур
syngui package -f aur                  # PKGBUILD + .SRCINFO для <имя>-git и <имя>-bin
syngui package -f ipa                  # неподписанный .ipa
syngui keygen -p syncook               # релизный ключ Android в ~/.secrets/syncook-android
syngui init                            # заготовка [package.metadata.syngui] в Cargo.toml
```

Приложение выбирается `-p <крейт>`, иначе — крейт текущего каталога, иначе — единственный крейт
рабочего пространства с `[package.metadata.syngui]`.

Профили: по умолчанию `fast-release` (если в Cargo.toml его нет — задаётся через `--config`),
`--release` — LTO для публикации, `--debug` — dev. `package` всегда собирает `--release`;
`--fast` — проверить упаковку быстро.

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
icon = "assets/icon.svg"           # SVG значка
bin = "syncook"                    # программа, если их несколько
desktop = "data/syncook.desktop"   # ярлык Linux
description_en = "Family recipes"  # описание для AUR (иначе package.description)
languages = ["ru", "en"]           # iOS: первый — основной

[package.metadata.syngui.android]
id = "kz.syn.syncook"              # если отличается от общего id
lib = "syncook"                    # android.app.lib_name (по умолчанию [lib] name)
features = ["android"]             # возможности cargo (по умолчанию ["android"], без default)
default_features = false
abis = ["arm64-v8a"]               # arm64-v8a, armeabi-v7a, x86_64
min_sdk = 30
target_sdk = 35
version_code = 3                   # по умолчанию из версии: 1.2.3 → 1002003
permissions = ["INTERNET"]         # короткие или полные имена
strip = false                      # оставить символы в .so (по умолчанию strip, кроме --debug)
ffmpeg = true                      # статический FFmpeg из syngui/android-deps/ffmpeg/<abi>
foreground_service = "quit_smoking_timer"  # SynGuiForegroundService (specialUse) + разрешения
background = "#FFF8F0"             # фон окна до первого кадра
orientation = "portrait"
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
depends = ["gcc-libs", "fontconfig", "libxkbcommon", "wayland", "vulkan-icd-loader"]
optdepends = ["ffmpeg: видео"]
makedepends = ["nasm"]             # для -git, к cargo git clang pkgconf
features = ["desktop"]
files = ["data/sounds:usr/share/syncook/sounds"]   # доп. файлы пакета «откуда:куда»
```

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
| `SYNPORT` | путь к synport для `run -d phone` |
