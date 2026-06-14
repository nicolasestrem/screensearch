"""Build the quality sidecar as a standalone Windows executable."""

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent

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
        "paddleocr",
        "--collect-all",
        "sentence_transformers",
        str(ROOT / "app.py"),
    ],
    cwd=ROOT,
    check=True,
)
