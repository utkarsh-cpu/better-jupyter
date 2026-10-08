#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$project_dir"
python_exe="$HOME/.venvs/kernelledger-gpu/bin/python"
"$python_exe" -m pip install --no-deps -e "$project_dir"
"$python_exe" - <<'PY'
import torch
print('PyTorch:', torch.__version__, 'HIP:', torch.version.hip)
assert torch.cuda.is_available(), 'No GPU exposed to PyTorch'
print(torch.cuda.get_device_properties(0))
a = torch.randn(1024, 1024, device='cuda:0')
result = a @ a.T
torch.cuda.synchronize()
assert torch.isfinite(result).all().item()
print('GPU matrix multiplication: PASS')
PY
"$python_exe" -I src/kernelledger/gpu/backends/probe.py cuda:0 | "$python_exe" -c 'import json,sys; report=json.load(sys.stdin); print(json.dumps(report,sort_keys=True)); assert report["status"] == "experimental", report'
"$python_exe" examples/gpu/portable_training.py --device cuda:0 --out "${1:-artifacts/wsl-8060s-donor-exit}"
"$python_exe" -m pytest tests/test_gpu_artifacts.py tests/test_gpu_portable.py tests/test_gpu_capabilities.py -q
