"""Build the quality sidecar as a standalone Windows executable."""

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
OCR_METADATA_PACKAGES = (
    "imagesize",
    "opencv-contrib-python",
    "pyclipper",
    "pypdfium2",
    "python-bidi",
    "shapely",
)

metadata_args = [
    argument
    for package in OCR_METADATA_PACKAGES
    for argument in ("--copy-metadata", package)
]

subprocess.run(
    [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        "--name",
        "screensearch-ai-sidecar",
        "--collect-all",
        "paddle",
        "--collect-all",
        "paddleocr",
        "--collect-all",
        "paddlex",
        "--collect-all",
        "sentence_transformers",
        *metadata_args,
        str(ROOT / "app.py"),
    ],
    cwd=ROOT,
    check=True,
)
