# RealPLKSR 2x

RealPLKSR 2x enlarges textures using AI. Select it in the application's texture settings or preview candidates. It runs locally on CPU and is slower than the other filters. Start with sharpening set to zero and compare the preview before applying it broadly.

Release archives include the model and everything needed to run it. Keep the complete `ai` directory alongside the executable. No internet connection or Python installation is needed to use this mode. If the application reports missing resources, extract the complete `ai` directory from the release archive again.

## Credits and licenses

This mode uses the MIT-licensed RealPLKSR 2x MSSIM-pretrain model by Dongheon Lee et al. See `RealPLKSR-LICENSE` for its license and the included `onnxruntime-LICENSE` and `onnxruntime-ThirdPartyNotices.txt` for runtime licenses and notices.

- [Original model](https://github.com/dslisleedh/PLKSR) and [checkpoint](https://github.com/dslisleedh/PLKSR/issues/4).
- [ONNX version](https://huggingface.co/darktable-org/upscale-realplksr-onnx/tree/0bb636eeeed5fffd256571c3cc440d9fdf62467e).
