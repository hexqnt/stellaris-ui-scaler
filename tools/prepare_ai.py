"""Готовит проверенные веса и CPU runtime для разработки и выпуска."""

import argparse
import hashlib
import shutil
import sys
import urllib.request
from pathlib import Path

import onnx
import onnxruntime


REVISION = "0bb636eeeed5fffd256571c3cc440d9fdf62467e"
SOURCE_SHA256 = "d7abb65092f3808d3aa255ffdab42b1d672883915d90adbdd321204168b9f293"
MODEL_SHA256 = "8fb923d0b1ad09d495c7b97c6046277c6f0c6c8d59f5a9c86e4bab707445f20f"
URL = f"https://huggingface.co/darktable-org/upscale-realplksr-onnx/resolve/{REVISION}/onnx/model_x2.onnx"


def prepare(destination: Path, source: Path | None):
    destination.mkdir(parents=True, exist_ok=True)
    target = destination / "realplksr-2x.onnx"
    if not target.exists() or hashlib.sha256(target.read_bytes()).hexdigest() != MODEL_SHA256:
        print("Preparing RealPLKSR 2x weights...", flush=True)
        data = source.read_bytes() if source else urllib.request.urlopen(URL, timeout=120).read()
        if hashlib.sha256(data).hexdigest() != SOURCE_SHA256:
            raise ValueError("RealPLKSR download checksum mismatch")
        model = onnx.load_model_from_string(data)
        # Меняются только внешние размеры; операции и веса исходного графа сохраняются.
        for value in [*model.graph.input, *model.graph.output]:
            for dimension, name in zip(value.type.tensor_type.shape.dim[2:], ["height", "width"]):
                dimension.ClearField("dim_value")
                dimension.dim_param = name if value.name == "input" else f"output_{name}"
        onnx.checker.check_model(model)
        data = model.SerializeToString()
        if hashlib.sha256(data).hexdigest() != MODEL_SHA256:
            raise ValueError("Prepared RealPLKSR checksum mismatch; use requirements-ai.txt")
        temporary = target.with_suffix(".tmp")
        temporary.write_bytes(data)
        temporary.replace(target)

    package = Path(onnxruntime.__file__).parent
    if sys.platform == "win32":
        library = package / "capi" / "onnxruntime.dll"
        name = library.name
    elif sys.platform.startswith("linux"):
        library = next((package / "capi").glob("libonnxruntime.so.*"))
        name = "libonnxruntime.so"
    else:
        raise ValueError("AI packaging currently supports Windows and Linux only")
    shutil.copy2(library, destination / name)
    for notice in ["LICENSE", "ThirdPartyNotices.txt"]:
        shutil.copy2(package / notice, destination / f"onnxruntime-{notice}")
    root = Path(__file__).resolve().parent.parent
    shutil.copy2(root / "assets" / "ai" / "RealPLKSR-LICENSE", destination)
    shutil.copy2(root / "assets" / "ai" / "README.md", destination / "RealPLKSR-README.md")
    print(f"RealPLKSR 2x and ONNX Runtime {onnxruntime.__version__} ready in {destination}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Prepare verified RealPLKSR 2x weights and CPU runtime")
    parser.add_argument("--output", type=Path, default=Path("ai"))
    parser.add_argument("--source", type=Path, help="Use an already downloaded original ONNX file")
    args = parser.parse_args()
    prepare(args.output, args.source)
