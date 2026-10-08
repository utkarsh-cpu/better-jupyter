"""Read-only target identity. Credentials and machine paths are never tracked."""

import hashlib
import os
import platform
import uuid
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Literal

TargetKind = Literal["local-windows", "local-linux", "wsl", "ssh-linux"]


@dataclass(frozen=True)
class Target:
    identity: str
    kind: TargetKind
    os: str
    architecture: str
    kernel: str
    distribution: str | None = None

    def describe(self) -> dict[str, str | None]:
        return asdict(self)


def local_target() -> Target:
    system, release = platform.system(), platform.release()
    kind: TargetKind
    if system == "Windows":
        kind = "local-windows"
    elif system == "Linux":
        kind = (
            "wsl"
            if "microsoft" in release.lower() or os.environ.get("WSL_DISTRO_NAME")
            else "local-linux"
        )
    else:
        raise ValueError("unsupported execution target OS: " + system)
    # Local identity is an inventory hint, never a remote authentication credential.
    identity = hashlib.sha256(
        (platform.node() + "\0" + kind + "\0" + os.environ.get("WSL_DISTRO_NAME", "")).encode()
    ).hexdigest()
    return Target(
        identity,
        kind,
        system,
        platform.machine(),
        release,
        os.environ.get("WSL_DISTRO_NAME") if kind == "wsl" else None,
    )


def runtime_identity(repository: str, worktree: Path, target: Target, generation: int) -> str:
    if type(generation) is not int or generation < 0:
        raise ValueError("invalid runtime generation")
    fields = (
        repository,
        os.path.normcase(str(Path(worktree).resolve())),
        target.identity,
        str(generation),
    )
    return str(uuid.uuid5(uuid.NAMESPACE_URL, "\0".join(fields)))
