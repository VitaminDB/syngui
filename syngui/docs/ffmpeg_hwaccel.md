# syngui/video — Hardware-accelerated decoding

> Версия syngui: master · feature `ffmpeg` (обязательно), без отдельной
> hwaccel-feature — всё через `ffmpeg-next 8.1 ffi`.

## Что делает

`syngui::video::HwAccel` — enum выбора платформенного HW-decoder'а:

| Вариант | Платформа | AVHWDeviceType | hw pix_fmt |
|---|---|---|---|
| `None` | — | — | — (sw-decode) |
| `Auto` | по `cfg!(target_os)` | переадресует на платформенный дефолт | |
| `Vaapi` | Linux (Intel/AMD/Mesa-обёртка NVIDIA) | `AV_HWDEVICE_TYPE_VAAPI` | `AV_PIX_FMT_VAAPI` |
| `Nvdec` | NVIDIA любая | `AV_HWDEVICE_TYPE_CUDA` | `AV_PIX_FMT_CUDA` |
| `VideoToolbox` | macOS | `AV_HWDEVICE_TYPE_VIDEOTOOLBOX` | `AV_PIX_FMT_VIDEOTOOLBOX` |
| `D3D11Va` | Windows ≥ 8 | `AV_HWDEVICE_TYPE_D3D11VA` | `AV_PIX_FMT_D3D11` |
| `Dxva2` | Windows 7+ | `AV_HWDEVICE_TYPE_DXVA2` | `AV_PIX_FMT_DXVA2_VLD` |
| `Vulkan` | Linux/Windows (с Vulkan-driver-decode) | `AV_HWDEVICE_TYPE_VULKAN` | `AV_PIX_FMT_VULKAN` |

При выборе ≠ `None`/`Auto`-в-no-op-режиме декодерный поток пытается
`av_hwdevice_ctx_create`, вешает device context на `AVCodecContext::
hw_device_ctx`, ставит `get_format` callback, который сообщает
libavcodec выбрать соответствующий hw pixel format. После декода
каждый кадр приходит в HW-surface; декодерный поток делает
`av_hwframe_transfer_data` обратно в системную память (обычно
`NV12`/`P010`), и уже это идёт в swscale → RGBA → texture.

**Это не zero-copy GPU pipeline.** Сам декод выполняется аппаратно
(существенный выигрыш для H.264/HEVC/AV1 на 720p–2160p), но кадр
дополнительно копируется CPU↔GPU. Полный zero-copy через DMA-BUF
(VAAPI ↔ Vulkan) / external memory (CUDA ↔ Vulkan) → wgpu — отдельная
задача, см. `TASK.md`.

## API

```rust
use syngui::video::{HwAccel, VideoPlayer};

// Обычно — Auto: на Linux подтянет VAAPI, на macOS — VideoToolbox,
// на Windows — D3D11Va.
let player = VideoPlayer::open_with_hwaccel("/path/to/file.mp4", HwAccel::Auto)?;

// Вручную:
let player = VideoPlayer::open_with_hwaccel("rtsp://…", HwAccel::Nvdec)?;

// SW-decode (как раньше):
let player = VideoPlayer::open("/path/to/file.mp4")?; // == HwAccel::None
```

Сам `VideoView`-виджет принимает уже готовый `Arc<Mutex<VideoPlayer>>`
— hwaccel-выбор делается на этапе `VideoPlayer::open_with_hwaccel(...)`,
дальше виджету всё равно sw или hw был источник кадров.

## Fallback и логирование

`HwContext::try_init` возвращает `Err`, если соответствующий драйвер
не установлен или устройство не найдено (`av_hwdevice_ctx_create`
вернул отрицательный код). В этом случае декодер тихо продолжает
в sw-режиме, в логи пишется warn:

```
WARN  hwaccel: init vaapi упал, fallback на sw: av_hwdevice_ctx_create(vaapi) вернул -22
```

Если HW init успешен, но конкретный кодек не умеет нужный hwaccel
(например, MPEG-4 + VAAPI на старых драйверах), `get_format` callback
вернёт первый sw-формат из предложенного списка — декод пройдёт в
software-режиме для этого источника, без падений.

## Требования по платформе

- **Linux + VAAPI**: пакет `libva` + драйверы (`intel-media-va-driver`
  / `mesa-va-drivers` / `libva-vdpau-driver`). Проверка:
  ```sh
  vainfo  # должен показать non-empty список профилей
  ls /dev/dri/renderD128   # должен существовать
  ```
- **NVIDIA + NVDEC**: установленный NVIDIA-драйвер с включённым
  `nvdecode` (default в современных драйверах). `nvidia-smi dmon -s d`
  покажет ненулевую утилизацию декодера.
- **macOS + VideoToolbox**: ничего отдельно ставить не нужно,
  включено системно.
- **Windows + D3D11VA**: рабочая GPU-карта с современным драйвером
  (DXGI 1.2+).

## Demo

`app/widget_gallery_mss/src/sections/ffmpeg_video.rs` — Dropdown
«HW: …» рядом с полем path/URL. По умолчанию выбран `auto`. При
смене значения **новый плеер** открывается на следующий клик
«Открыть» — старый плеер дропается.

## Чего тут нет (followup)

- **Zero-copy GPU import** — рендеринг HW-surface напрямую в
  `wgpu::Texture` без CPU-копии. Требует DMA-BUF/external-memory
  Vulkan-import и отдельных code path под платформы.
- **Вынос `syngui/src/video/` в отдельный sub-crate `syngui-video`** —
  изоляция тяжёлой опциональной зависимости `ffmpeg-next` от ядра
  виджетов. Сейчас она под feature `ffmpeg`, что достаточно для CI,
  но раздельный crate был бы чище.
