"""Active WSL native GPU feasibility probe; no dump or restore is attempted.

Run with the WSL GPU interpreter. Optional --out writes the evidence record.
"""

import argparse
import datetime
import hashlib
import json
import os
import platform
import shutil
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if platform.system() != "Linux" or "microsoft" not in platform.release().lower():
        parser.error("this probe is specific to the WSL DXG target")

    import torch

    if not torch.version.hip:
        parser.error("this probe requires the AMD ROCm PyTorch environment")
    value = torch.arange(1024, dtype=torch.float32, device="cuda:0")
    torch.cuda.synchronize()
    assert value.sum().item() == 523776
    properties = torch.cuda.get_device_properties(0)
    handles = []
    for fd in Path("/proc/self/fd").iterdir():
        try:
            target = os.readlink(fd)
        except OSError:
            continue
        if target.startswith("/dev/"):
            handles.append(target)
    checks = {
        "gpu_computation": "passed",
        "dxg_present": Path("/dev/dxg").exists(),
        "kfd_present": Path("/dev/kfd").exists(),
        "amdgpu_kernel_module_present": Path("/sys/module/amdgpu").exists(),
        "kfd_topology_present": Path("/sys/class/kfd").exists(),
        "live_gpu_process_device_handles": sorted(set(handles)),
        "criu_binary": shutil.which("criu"),
    }
    apt_policy = subprocess.run(
        ["apt-cache", "policy", "criu"], capture_output=True, text=True, timeout=10
    )
    checks["criu_apt_candidate"] = next(
        (
            line.strip().split(":", 1)[1].strip()
            for line in apt_policy.stdout.splitlines()
            if line.strip().startswith("Candidate:")
        ),
        "unknown",
    )
    kernel_config = subprocess.run(
        ["zcat", "/proc/config.gz"], capture_output=True, text=True, timeout=10
    )
    checks["kernel_config_readable"] = kernel_config.returncode == 0
    checks["kernel_checkpoint_config"] = [
        line
        for line in kernel_config.stdout.splitlines()
        if line.startswith("CONFIG_CHECKPOINT_RESTORE=")
        or line.startswith("# CONFIG_CHECKPOINT_RESTORE ")
    ]
    reasons = []
    if not checks["kfd_present"]:
        reasons.append("No /dev/kfd: the AMD CRIU plugin cannot reach its required KFD interface")
    if not checks["amdgpu_kernel_module_present"]:
        reasons.append("No loaded amdgpu kernel driver; computation uses Windows GPU state via DXG")
    if not checks["criu_binary"]:
        reasons.append("CRIU is not installed; apt candidate: " + checks["criu_apt_candidate"])
    driver_unsupported = not checks["kfd_present"] or not checks["amdgpu_kernel_module_present"]
    report = {
        "schema": "kernelledger.experimental.native-feasibility/1",
        "recorded_at": datetime.datetime.now(datetime.UTC).isoformat(),
        "target_alias": os.environ.get("WSL_DISTRO_NAME", "Ubuntu-24.04"),
        "target_kind": "wsl",
        "kernel": platform.release(),
        "python": platform.python_version(),
        "torch": torch.__version__,
        "hip": torch.version.hip,
        "gpu": properties.name,
        "architecture": properties.gcnArchName,
        "checks": checks,
        "native_process_status": "unsupported"
        if driver_unsupported
        else ("unavailable" if not checks["criu_binary"] else "unqualified"),
        "native_qualified": False,
        "native_image_created": False,
        "native_resume_tested": False,
        "reasons": reasons or ["Prerequisites alone do not qualify capture and resume"],
        "portable_fallback_evidence": "wsl-8060s-portable-2026-10-07.json",
        "sources": [
            "https://github.com/checkpoint-restore/criu/blob/criu-dev/Documentation/criu-amdgpu-plugin.txt",
            "https://github.com/checkpoint-restore/criu/blob/criu-dev/plugins/amdgpu/README.md",
            "https://github.com/ROCm/rocm-cli/blob/main/docs/wsl.md",
        ],
    }
    report["evidence"] = hashlib.sha256(json.dumps(report, sort_keys=True).encode()).hexdigest()
    output = json.dumps(report, indent=2) + "\n"
    if args.out:
        args.out.write_text(output, encoding="utf-8")
    print(output, end="")


if __name__ == "__main__":
    main()
