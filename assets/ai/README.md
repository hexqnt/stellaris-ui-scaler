# RealPLKSR 2x

The optional RealPLKSR 2x algorithm uses the MIT-licensed `2x_realplksr_mssim_pretrain` weights by Dongheon Lee et al. This is the MSSIM-pretrain model, without GAN fine-tuning.

- Architecture and checkpoint source: https://github.com/dslisleedh/PLKSR and https://github.com/dslisleedh/PLKSR/issues/4
- ONNX conversion: https://huggingface.co/darktable-org/upscale-realplksr-onnx/tree/0bb636eeeed5fffd256571c3cc440d9fdf62467e
- Original ONNX SHA-256: `d7abb65092f3808d3aa255ffdab42b1d672883915d90adbdd321204168b9f293`
- Prepared ONNX SHA-256: `8fb923d0b1ad09d495c7b97c6046277c6f0c6c8d59f5a9c86e4bab707445f20f`

`tools/prepare_ai.py` changes only the graph's input/output height and width declarations to symbolic dimensions. The original graph operations and weights are preserved. Inference uses float32 RGB tensors in NCHW layout, with values in [0, 1], and produces a native 2x image.

The application uses CPU ONNX Runtime, lazily loads independent sessions for active build tasks, and retains a separate session across preview requests in the same window. Rayon concurrency adapts to the CPU parallelism available to the process and the current Rayon pool (including RAYON_NUM_THREADS). Each batch also respects the image-buffer memory budget. The CPU budget is divided among active tasks for ONNX inference; a single active task can use the entire budget. Build sessions are reused across batches with the same thread budget; sessions are recreated when that budget changes. Neural processing within each session is serialized; preview candidates run concurrently and appear as they finish. Large images use 192-pixel cores with 32 pixels of context on each side. Group normalization depends on image contents, so tiled inference can differ from whole-image inference. Alpha is resampled separately; fully transparent RGB is extended from visible pixels before inference. Final resizing uses Lanczos3 and the profile's color-space, alpha and sharpening settings. Native intermediate images are subject to the same 64-million-pixel limit as other image buffers.

The model and runtime are loaded only when RealPLKSR is used. They are distributed in the `ai` directory beside release executables. For source builds, prepare the directory at the repository root or set `STELLARIS_UI_SCALER_AI_DIR` to its absolute path. No model download occurs during a build or preview.
