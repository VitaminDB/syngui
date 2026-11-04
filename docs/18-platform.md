# Платформенные службы: выбор фото/видео, «Поделиться», открыть файл

Модуль `syngui::platform` — системные диалоги, которые нельзя нарисовать
виджетами: выбор медиа из галереи, отправка текста/файла в другое приложение
и открытие файла во внешнем просмотрщике (например, для печати PNG/PDF).

```rust
use syngui::platform::{self, MediaKind, PickedFile};

// Выбор: не блокирует, колбэк приходит в UI-потоке — сигналы трогать можно.
platform::pick_media(MediaKind::Image, move |r: Result<Option<PickedFile>, String>| match r {
    Ok(Some(f)) => photo.set(Some(f.path)),   // f.name, f.mime тоже есть
    Ok(None) => {}                            // пользователь отменил
    Err(e) => error.set(e),
});

platform::share_text("Борщ", &recipe_text)?;
platform::share_file(Path::new("/…/borsch.png"), "image/png", "Борщ")?; // mime "" — по расширению
platform::open_path(Path::new("/…/card.pdf"))?;
```

`platform::has_native_share()` — есть ли настоящее «Поделиться» (Android) или
`share_*` копирует в буфер обмена (Linux): по нему приложение выбирает подпись
кнопки и текст подтверждения («Скопировано»).

## Android

| Функция | Как сделано |
|---|---|
| `pick_media` | `MediaStore.ACTION_PICK_IMAGES` (системный фото-пикер; Android 13+ или 11–12 с SDK extension ≥ 2), иначе `ACTION_GET_CONTENT`. Результат (content://) копируется в `cacheDir/syngui-picked/`, `path` — эта копия. |
| `share_text` | `ACTION_SEND` `text/plain` + `Intent.createChooser` |
| `share_file` | `ACTION_SEND` с `EXTRA_STREAM` + chooser, URI — через FileProvider |
| `open_path` | `ACTION_VIEW` с типом по расширению |

**Фото HEIC/HEIF/AVIF** (по умолчанию так снимают Samsung и Pixel) и JPEG с
EXIF-поворотом перекодируются в JPEG с применённым поворотом: иначе `Image`
syngui их не покажет (или покажет боком). Видео копируется как есть.

Копия в кэше может быть удалена системой — приложение переносит файл к себе
(`internal_data_path`) и удаляет копию.

Результат активити берётся через `ActivityResultRegistry` самой GameActivity
(она `AppCompatActivity`), поэтому отдельной Activity в манифесте не нужно.
Java-часть — `syngui/src/app/android/SynGuiPlatformHandler.java` в DEX
фреймворка (`scripts/build-android-dex.sh`).

### Манифест приложения: FileProvider

Чтобы отдать свой файл другому приложению (`share_file`, `open_path`),
нужен content://-URI. Добавьте провайдер (authority строго
`<пакет>.syngui.files`):

```xml
<provider
    android:name="androidx.core.content.FileProvider"
    android:authorities="${applicationId}.syngui.files"
    android:exported="false"
    android:grantUriPermissions="true">
    <meta-data
        android:name="android.support.FILE_PROVIDER_PATHS"
        android:resource="@xml/syngui_file_paths" />
</provider>
```

`res/xml/syngui_file_paths.xml`:

```xml
<paths>
    <files-path name="files" path="." />
    <cache-path name="cache" path="." />
    <external-files-path name="external_files" path="." />
    <external-cache-path name="external_cache" path="." />
</paths>
```

`androidx.core` приходит с `androidx.appcompat`. Без провайдера (или для файла
вне этих каталогов) файл копируется в общую папку `Download` через MediaStore и
отдаётся оттуда — работает, но оставляет копию у пользователя.

Galaxy Fold при складывании меняет размер и плотность экрана — в `configChanges`
активности стоит указать
`orientation|screenSize|smallestScreenSize|screenLayout|density|uiMode|…`,
иначе Activity пересоздаётся (в том числе пока открыт пикер). Образец —
`syncook/android/app/src/main/AndroidManifest.xml`.

## Linux (KDE, synshell, прочие)

| Функция | Как сделано |
|---|---|
| `pick_media` | портал `org.freedesktop.portal.FileChooser.OpenFile` с MIME-фильтрами (фича `portal`); нет портала — `kdialog` (в KDE) или `zenity` |
| `share_text` | текст в буфер обмена (`Ok` = скопировано) |
| `share_file` | файл в буфер обмена (`text/uri-list` — вставляется в мессенджер или файловый менеджер как файл) + показ в файловом менеджере (`org.freedesktop.FileManager1.ShowItems`, иначе `xdg-open` каталога) |
| `open_path` | `xdg-open`; во flatpak (или без `xdg-open`) — портал `OpenURI.OpenFile` |

В synshell `FileChooser` портала реализует Проводник оболочки (`synfiles
--choose`), в KDE — xdg-desktop-portal-kde. Фича:

```toml
syngui = { …, features = ["portal", "clipboard"] }
```

## Остальные платформы

`Err("не поддерживается")` (у `pick_media` — в колбэке), без паники.

## Проверка

`cargo run -p syngui --example platform_probe --features portal -- pick image`
открывает системный диалог и печатает результат (`Ok(None)` — отмена); также
`share-text ТЕКСТ`, `share-file ПУТЬ`, `open ПУТЬ`.
