# Stellaris UI Scaler

**English** | [Русский](README.ru.md)

![Stellaris UI Scaler poster](poster/poster.png)

Create a mod with a larger Stellaris interface: scale UI elements, textures, and fonts without changing the game files. Includes a graphical interface and a CLI.

## Installation

Download a **Windows or Linux** build from the repository's **Releases**, extract the archive, and run `stellaris-ui-scaler` (`stellaris-ui-scaler.exe` on Windows). Keep the included `profiles` and `ai` directories alongside the executable.

Prebuilt binaries require an x86-64-v3 CPU with AVX2/FMA. Linux builds require glibc 2.39 or newer and X11/Wayland and OpenGL libraries for the GUI.

## Create a mod

1. In **Project**, select your Stellaris installation directory and click **Scan assets**. Use **Find game** or enter the path manually if needed.
2. Set **Scale**, such as `2.0` to double the size, and **Output directory**, where the mod will be stored.
3. Optionally, adjust **Textures**, **Fonts**, and the name and picture in **Mod details**. Click **Save profile** to keep your settings.
4. Click **Build mod** and wait for it to finish.
5. In **Mod details**, check **Mods directory** (the `mod` folder in your Stellaris user data) and click **Install mod**. Keep **Output directory** in place: the installed mod refers to it.
6. Restart the Stellaris launcher, enable the generated mod in your playset, and set **UI Scaling = 1.0** in the game.

After changing settings, rebuild and install the mod again.

## Fonts and other UI mods

In **Fonts**, select **Sharp text (TrueType)** for sharper text in supported fonts, or **Original fonts** to preserve the original typefaces. Adjust **Text size** separately if needed. Texture previews help you check scaling and sharpening before building.

In **Textures**, click **Search by image…**, use **Paste image** (or Ctrl+V outside a text field), drop an image file into the search window, or enter its path and click **Load image**. The background search uses multiple threads and shows results in batches of 20 resources with thumbnails; scrolling down automatically loads subsequent batches without a 60-result limit; click a result to open its regular preview and processing rules. Ranking combines dimension similarity (50%), palette and spatial color similarity (35%), and shape (15%). Dimensions consider both the source frame and the frame scaled by its rule in the current profile; sprite frames and variants without transparent margins are compared separately. Palette comparison uses a smoothed RGB histogram and Hellinger distance, including brightness; brightness normalization applies only to shape. The first query builds an in-memory index, reused until a new scan or source change. Unreadable textures are skipped with a count. Tightly cropped icons and buttons work best; composite panels, overlaid text, and arbitrary regions inside atlases may rank poorly.

**Scan assets** detects enabled UI mods from the launcher's exported configuration. In **Project → Source UI mods**, uncheck any mods you want to exclude, then scan again. To use only the base game's interface, uncheck all rows.

Keep source mods enabled in the launcher and load the generated mod after them. Mods are processed in load order; later mods override files with matching paths. Other UI mods may conflict, so check the result in the game.

After changing your playset, refresh the launch configuration through the launcher and scan again. Only unpacked mods are supported; archived mods and `replace_path` overrides affecting UI resources are not supported. For a custom user data location, set **Source UI mods → User data directory**.

## Install from source

Requires Rust nightly and a system build toolchain, plus the GUI libraries listed above on Linux. From the repository root:

```bash
cargo install --path . --locked
stellaris-ui-scaler
```

For a CLI-only installation, add `--no-default-features`. Run `stellaris-ui-scaler --help` for available commands.

## RealPLKSR 2x

Choose **RealPLKSR 2x** in the texture algorithm or preview candidates, or load `profiles/4k_realplksr_2x.toml`. Disable **Auto algorithm** when selecting it as the default. Category, directory and asset rules can also select it. CLI builds accept `--algorithm realplksr-2x`; TOML profiles use `algorithm = "realplksr_2x"` under `[textures]`.

This mode uses the MIT-licensed RealPLKSR 2x MSSIM-pretrain model and runs locally on CPU. It is slower than the resampling filters. Each sprite frame is processed separately, alpha is resampled without neural inference, and the native 2x result is resized to the requested scale with Lanczos3. Downsampling uses Lanczos3 directly. Start with sharpening set to zero and compare the preview before applying the model broadly. Large images are processed in tiles; normalization can cause differences from whole-image inference.

Release archives include the required resources. For source builds, run the following in a Python virtual environment from the repository root:

```bash
python -m pip install -r tools/requirements-ai.txt
python tools/prepare_ai.py
```

Keep the generated `ai` directory in the working directory or beside the executable. Alternatively, set `STELLARIS_UI_SCALER_AI_DIR` to its absolute path. Python is only used to prepare the resources; normal use is offline and does not require Python. The preparation script verifies the model checksum and includes model/runtime licenses. To run the tests that use the actual model: `cargo test --all-features realplksr -- --ignored`. The benchmark command compares resampling filters by default; add `--algorithm realplksr-2x` to measure this model.

## Credits

Inspired by [Hayneken's mod](https://steamcommunity.com/sharedfiles/filedetails/?id=3801492978).
