#!/usr/bin/env bash
# Ubuntu 24.04 WSL2 / Radeon 8060S qualification environment.
set -euo pipefail
case "${1:-}" in
  system)
    test "$(id -u)" = 0
    python3 - <<'PY'
from pathlib import Path
p = Path('/etc/apt/sources.list.d/ubuntu.sources')
backup = Path('/var/backups/ubuntu.sources.before-gpu-setup')
if not backup.exists():
    backup.write_bytes(p.read_bytes())
p.write_text(p.read_text().replace('http://archive.ubuntu.com/', 'https://archive.ubuntu.com/').replace('http://security.ubuntu.com/', 'https://security.ubuntu.com/'))
PY
    apt-get -o Acquire::Retries=2 -o Acquire::https::Timeout=30 update
    DEBIAN_FRONTEND=noninteractive apt-get install -y python3-venv python3-pip curl ca-certificates libatomic1 libquadmath0 libnuma1 libnuma-dev
    download_dir=$(mktemp -d /tmp/kl-rocdxg-XXXXXX)
    curl --fail --location --retry 2 --output "$download_dir/rocdxg-roct_1.2.1_amd64.deb" https://github.com/ROCm/librocdxg/releases/download/v1.2.1/rocdxg-roct_1.2.1_amd64.deb
    apt-get install -y "$download_dir/rocdxg-roct_1.2.1_amd64.deb"
    ldconfig
    ;;
  environment)
    test "$(id -u)" != 0
    python3 -m venv "$HOME/.venvs/kernelledger-gpu"
    "$HOME/.venvs/kernelledger-gpu/bin/python" -m pip install --upgrade pip
    "$HOME/.venvs/kernelledger-gpu/bin/python" -m pip install --index-url https://pypi.org/simple --extra-index-url https://repo.amd.com/rocm/whl-multi-arch/ 'torch[device-gfx1151]==2.12.0+rocm7.14.0' 'rocm[libraries,device-gfx1151]==7.14.0' 'numpy==2.5.2' pytest hypothesis jsonschema pandas pyarrow matplotlib ruff
    "$HOME/.venvs/kernelledger-gpu/bin/python" -m pip check
    ;;
  *) echo 'Usage: setup-wsl-gpu.sh system|environment' >&2; exit 2 ;;
esac
