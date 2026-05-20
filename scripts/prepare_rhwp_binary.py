from __future__ import annotations

import os
import lzma
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor" / "rhwp"
BIN_DIR = ROOT / "src" / "kdsnr_hwp_toolkit" / "bin"


def main() -> None:
    exe_name = "rhwp.exe" if os.name == "nt" else "rhwp"
    env = _cargo_env()
    env.setdefault("CARGO_PROFILE_RELEASE_STRIP", "symbols")
    subprocess.run(
        ["cargo", "build", "--release", "--manifest-path", str(VENDOR / "Cargo.toml")],
        check=True,
        cwd=ROOT,
        env=env,
    )
    built = VENDOR / "target" / "release" / exe_name
    if not built.exists():
        raise FileNotFoundError(f"rhwp build did not produce {built}")
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    target = BIN_DIR / f"{exe_name}.xz"
    raw_target = BIN_DIR / exe_name
    if raw_target.exists():
        raw_target.unlink()
    tmp = BIN_DIR / f"{exe_name}.tmp"
    shutil.copy2(built, tmp)
    _strip_binary(tmp)
    if os.name != "nt":
        tmp.chmod(tmp.stat().st_mode | 0o755)
    with tmp.open("rb") as src, lzma.open(target, "wb", preset=9) as dst:
        shutil.copyfileobj(src, dst)
    tmp.unlink()
    print(target)


def _strip_binary(path: Path) -> None:
    for tool in ("strip", "llvm-strip"):
        exe = shutil.which(tool)
        if exe is None:
            continue
        try:
            subprocess.run([exe, str(path)], check=True)
            return
        except subprocess.CalledProcessError:
            continue


def _cargo_env() -> dict[str, str]:
    if not hasattr(os, "environb"):
        env = os.environ.copy()
        env.pop("_", None)
        return env
    env = {}
    for key_b, value_b in os.environb.items():
        try:
            key = key_b.decode()
            value = value_b.decode()
        except UnicodeDecodeError:
            continue
        env[key] = value
    env.pop("_", None)
    return env


if __name__ == "__main__":
    main()
