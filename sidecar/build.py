"""Build the quality sidecar as a standalone Windows executable."""

from pathlib import Path
import os
import shutil
import subprocess
import sys

# Python 3.12.0 ships a PEP 709 (inlined comprehension) code-generation bug that
# corrupts module-level loop/comprehension bindings in large modules once frozen
# by PyInstaller. It makes `import scipy.stats` abort with
# "NameError: name 'obj' is not defined" in scipy/stats/_distn_infrastructure.py
# (the docstring cleanup `for obj in [s for s in dir() ...]: ... ; del obj`),
# which takes down sentence-transformers and the whole sidecar at startup. The
# bug is fixed in later 3.12 patch releases (verified: 3.12.0 fails, 3.12.10
# works, same scipy + PyInstaller). Refuse to build a sidecar that would crash on
# every machine.
if sys.version_info[:2] == (3, 12) and sys.version_info[2] < 1:
    raise SystemExit(
        "Refusing to build the sidecar with Python "
        f"{'.'.join(map(str, sys.version_info[:3]))}: the 3.12.0 PEP 709 codegen "
        "bug makes the frozen scipy/sentence-transformers import crash at startup. "
        "Use a patched 3.12.x (>= 3.12.1; 3.12.10 verified) instead."
    )

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


def _refresh_bundled_msvc_runtime() -> None:
    """Replace the MSVC C runtime PyInstaller bundled with the host's current copy.

    PyInstaller collects whatever ``vcruntime140``/``msvcp140`` it first resolves
    on the build machine, which can be older than the toolset PyTorch's
    ``c10.dll`` was compiled against. PyTorch loads its DLLs with a restricted
    search (``LoadLibraryExW(..., 0x1100)``) that prefers the bundle's
    ``_internal`` directory over ``System32``, so a stale runtime makes
    ``import torch`` abort at startup with ``OSError: [WinError 1114]`` on every
    machine -- not just the build host. Refreshing the bundled copies from
    System32 (the same version the installer's vc_redist provides) keeps the
    bundle self-consistent. No-op off Windows (the sidecar also builds on Linux).
    """
    if os.name != "nt":
        return
    internal = ROOT / "dist" / "screensearch-ai-sidecar" / "_internal"
    system32 = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32"
    runtime_dlls = (
        "msvcp140.dll",
        "msvcp140_atomic_wait.dll",
        "vcruntime140.dll",
        "vcruntime140_1.dll",
    )
    for name in runtime_dlls:
        bundled = internal / name
        source = system32 / name
        # Only refresh runtimes PyInstaller actually bundled, and only when the
        # host provides a copy to replace them with.
        if bundled.exists() and source.exists():
            shutil.copy2(source, bundled)
            print(f"Refreshed bundled MSVC runtime from System32: {name}")


_refresh_bundled_msvc_runtime()
