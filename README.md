# main.frame

<!-- Badges -->
[![License](https://img.shields.io/badge/License-GPL%20v3-orange)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange)](https://www.rust-lang.org/)
[![Vulkan](https://img.shields.io/badge/Vulkan-1.0%2B-red)](https://www.vulkan.org/)
[![Windows](https://img.shields.io/badge/Windows-10%2B-blue)](https://www.microsoft.com/windows)
[![Linux](https://img.shields.io/badge/Linux-x86%20%2F%20ARM-green)](https://www.linux.org/)
[![macOS](https://img.shields.io/badge/macOS-Apple%20Silicon-lightgrey)](https://www.apple.com/macos/)
[![Version](https://img.shields.io/badge/Version-Beryllium%20v0.1-blueviolet)](https://github.com/halva89/main-frame-graphics/releases)
[![Author](https://img.shields.io/badge/Author-Halva-blue)](https://github.com/halva89)

---

## RU

### main.frame (далее MF) — бесплатный open-source графический движок на `Rust` с использованием `Vulkan`. ***COPYLEFT ЛИЦЕНЗИЯ***

main.frame — собственный, полностью контролируемый графический стек:
реальный рендер на Vulkan (тени, MSAA/FXAA), физика, анимации и эксклюзивный
**термальный режим** (тепловизор с сиянием и распределением тепла). Сцены
описываются текстовым форматом `.mff` — их можно читать и править руками.

Из чего состоит движок:

- **Рендеринг**: Vulkan (ash), единый пул геометрии на кадр, тени, MSAA и FXAA.
- **Отсечение невидимого**: view-frustum culling по AABB объектов.
- **Физика**: rapier3d — гравитация, упругие тела, стены и кучи кубов.
- **Анимация**: keyframe-система (позиция + цвет), плавная интерполяция.
- **Тепловизор**: температурная шкала от холодного синего до белого, аддитивный
  блюм, сияние, распределение тепла по объектам и шум датчика.
- **Кроссплатформенность**: Windows, Linux (x86_64 + ARM), macOS (Apple Silicon).

---

### Версии

Версии до выхода Beta обозначаются элементами таблицы Менделеева. После выхода
Beta — в честь гор. Все релизы можно посмотреть [здесь](https://github.com/halva89/main-frame-graphics/releases).

1. **Alpha 1: Uranium.** Создан первый треугольник и шейдеры. Архитектура не обозначена. [X] ВЫПОЛНЕНО
2. **Alpha 2: Gold.** Создан базовый парсер файлов `.mff`. Архитектура Rusty Wolf. [X] ВЫПОЛНЕНО
3. **Alpha 3.0: Lithium.** Полноценный парсер 2D-фигур (треугольник, круг, квадрат). В релизе — демо-сцена. Архитектура Red Wolf. [X] ВЫПОЛНЕНО
4. **Alpha 3.1: Lithium V2.** Анимации (alpha, bugs included). Полноценный парсер `.mff`: `animation`, остальные фигуры, настройки окна. Камера с калибровкой по вьюпорту. Выход с уничтожением ресурсов. [X] ВЫПОЛНЕНО
5. **Pre-beta: Beryllium.** Устранение багов, базовые 3D-фигуры. Многопоточный рендеринг и рефакторинг. [X] ВЫПОЛНЕНО
6. **Beta 1: McKinley.** Тени, реализация биндов. [/] В РАЗРАБОТКЕ
7. **Beta 2: BrowdPeek.** Логические элементы и `.mff` V3. [|] В ПОДГОТОВКЕ
8. **Beta 3: RozaPeek.** API для модов и UI. [-] В БУДУЩЕМ
9. **Pre-Release: Kongur.** Заморозка API, решение багов, полная кроссплатформенность. [-] В БУДУЩЕМ
10. **Release: Oxygen Everest.** Презентация, билдинг, система поддержки. [-] В БУДУЩЕМ

---

### Системные требования (последняя версия)

Оценено по реальным замерам release-бинарника (см. ниже) — движок очень лёгкий:
обычные сцены ~100 МБ RAM, стрессовая (1400 объектов) ~150 МБ, процесс занимает
единицы процентов одного ядра. Повторить можно установив в качестве основной сцены сцену stress-test.mff

**Минимальные:**
- Система: Windows 10 / Linux / macOS (главное — наличие Vulkan)
- Процессор: любое x86-64 или ARM64 ядро
- Оперативная память: 1 ГБ
- Видеокарта: поддержка Vulkan 1.0 (драйвер 2015+)
- Диск: ~100 МБ на установку

**Рекомендуемые:**
- Система: Windows 10/11, современный Linux или macOS
- Процессор: любой двухъядерный (разработка ведётся на Intel UHD)
- Оперативная память: 2 ГБ
- Видеокарта: любой Vulkan 1.0 драйвер

*Замеры (release, 12-ядерный CPU, интегрированная графика):* thermal — 100 МБ RAM / ~2% одного ядра; frame — 98 МБ / ~1%; stress (1400 объектов) — 153 МБ / ~5%.**

---

### Установка

1. Установите из [релизов](https://github.com/halva89/main-frame-graphics/releases) последнюю подходящую версию.
2. Распакуйте папку `release`.
3. Откройте папку и запустите `main_frame.exe <сцена.mff>` (например, `main_frame.exe thermal.mff`).

*ИЛИ (из исходников):*

1. Установите [Rust SDK](https://www.rust-lang.org/) и распакуйте исходники последней версии.
2. Перейдите в папку исходников.
3. Выполните `cargo build --release` или `cargo run --release`.

### Сборка на Linux / macOS

Проект кроссплатформенный (Win32-вызовы спрятаны под `#[cfg(windows)]`). Нужен драйвер
с поддержкой **Vulkan** и системные dev-библиотеки **winit** (крэйт `winit` тянет их при сборке).

**Debian / Ubuntu:**
```bash
sudo apt install build-essential libvulkan-dev libx11-dev libxrandr-dev libxinerama-dev \
  libxcursor-dev libxi-dev libxkbcommon-dev wayland-protocols libwayland-dev
cargo run --release
```

**Fedora:**
```bash
sudo dnf install gcc-c++ vulkan-loader-devel libX11-devel libXrandr-devel libXinerama-devel \
  libXcursor-devel libXi-devel libxkbcommon-devel wayland-devel wayland-protocols-devel
cargo run --release
```

**macOS** (Vulkan эмулируется через Metal — нужен MoltenVK):
```bash
brew install molten-vk glfw
export VK_ICD_FILENAMES="/opt/homebrew/share/vulkan/icd.d/MoltenVK_icd.json"  # при необходимости
cargo run --release
```

`glslangValidator` устанавливать не обязательно: `.spv`-шейдеры уже скомпилированы
и лежат в `shaders/`, а `build.rs` пересобирает их только если утилита есть в PATH.

---

## EN

### main.frame (hereinafter MF) — a free open-source graphics engine written in `Rust` using `Vulkan`. ***COPYLEFT LICENSE***

main.frame is a self-owned, fully controlled graphics stack: real Vulkan
rendering (shadows, MSAA/FXAA), physics, animation and an exclusive
**thermal mode** (thermal imaging with glow and heat distribution). Scenes are
written in the text `.mff` format — easy to read and edit by hand.

What the engine includes:

- **Rendering**: Vulkan (ash), a single geometry pool per frame, shadows, MSAA and FXAA.
- **Culling**: view-frustum culling based on object AABBs.
- **Physics**: rapier3d — gravity, elastic bodies, walls and stacks of cubes.
- **Animation**: keyframe system (position + color) with smooth interpolation.
- **Thermal mode**: temperature scale from cold blue to white, additive bloom,
  glow, heat distribution between objects and sensor noise.
- **Cross-platform**: Windows, Linux (x86_64 + ARM), macOS (Apple Silicon).

---

### Versions

Versions before Beta are named after elements of the periodic table; after Beta —
after mountains. You can view all releases [here](https://github.com/halva89/Snow-Graphics-System/releases).

1. **Alpha 1: Uranium.** The first triangle and shaders were created. Architecture not defined. [X] COMPLETED
2. **Alpha 2: Gold.** A basic parser for the `.mff` file format was created. Architecture: Rusty Wolf. [X] COMPLETED
3. **Alpha 3.0: Lithium.** A full-featured 2D shape parser (triangle, circle, square) is ready. The release includes a demo scene. Architecture: Red Wolf. [X] COMPLETED
4. **Alpha 3.1: Lithium V2.** Animations are ready (alpha, bugs included). Full-featured `.mff` parser: `animation`, other shapes, window settings. A camera with viewport calibration has been added. Exit with resource destruction. [X] COMPLETED
5. **Pre-beta: Beryllium.** Bug fixes, basic 3D shapes. Multithreaded rendering and refactoring. [X] COMPLETED
6. **Beta 1: McKinley.** Shadow creation, bind implementation. [/] IN DEVELOPMENT
7. **Beta 2: BrowdPeek.** Logical elements and `.mff` V3. [|] IN PREPARATION
8. **Beta 3: RozaPeek.** Mod API and UI. [-] IN THE FUTURE
9. **Pre-Release: Kongur.** API freeze, bug fixing, full cross-platform support. [-] IN THE FUTURE
10. **Release: Oxygen Everest.** Presentation, building, support system. [-] IN THE FUTURE

---

### System requirements (latest version)

Based on real measurements of the release binary (see below) — the engine is
very light: typical scenes use ~100 MB RAM, the stress scene (1400 objects)
~150 MB, and the process takes a few percent of a single core.

**Minimum:**
- OS: Windows 7+ / Linux / macOS (any platform with Vulkan support)
- CPU: any x86-64 or ARM64 core
- RAM: 512 MB
- GPU: Vulkan 1.0 support (driver 2015+)
- Disk: ~5 MB for installation

**Recommended:**
- OS: Windows 10/11, modern Linux or macOS
- CPU: any dual-core (development runs on Intel UHD)
- RAM: 1 GB
- GPU: any Vulkan 1.0 driver

*Measurements (release, 12-core CPU, integrated graphics):* thermal — 100 MB RAM / ~2% of one core; frame — 98 MB / ~1%; stress (1400 objects) — 153 MB / ~5%.**

---

### Installation

**Method 1 (binary):**
1. Download the latest suitable version from [releases](https://github.com/halva89/Snow-Graphics-System/releases).
2. Extract the `release` folder.
3. Open the folder and run `main_frame.exe <scene.mff>` (e.g. `main_frame.exe thermal.mff`).

**Method 2 (building from source):**
1. Install the [Rust SDK](https://www.rust-lang.org/) and unpack the latest sources.
2. Navigate to the source folder.
3. Run `cargo build --release` or `cargo run --release`.