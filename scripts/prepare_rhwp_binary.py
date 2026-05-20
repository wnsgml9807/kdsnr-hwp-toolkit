from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor" / "rhwp"
BIN_DIR = ROOT / "src" / "kdsnr_hwp_toolkit" / "bin"


def main() -> None:
    exe_name = "rhwp.exe" if os.name == "nt" else "rhwp"
    subprocess.run(
        ["cargo", "build", "--release", "--manifest-path", str(VENDOR / "Cargo.toml")],
        check=True,
        cwd=ROOT,
    )
    built = VENDOR / "target" / "release" / exe_name
    if not built.exists():
        raise FileNotFoundError(f"rhwp build did not produce {built}")
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    target = BIN_DIR / exe_name
    shutil.copy2(built, target)
    if os.name != "nt":
        target.chmod(target.stat().st_mode | 0o755)
    print(target)


if __name__ == "__main__":
    main()
