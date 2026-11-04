"""Build/verify an experimental local daemon archive; never downloads or deploys."""

import argparse
import hashlib
import json
import platform
import subprocess
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def source_snapshot() -> str:
    paths = [
        "desktop/Cargo.toml",
        "desktop/Cargo.lock",
        "desktop/crates/runtime/Cargo.toml",
        "desktop/crates/protocol/Cargo.toml",
        "desktop/crates/daemon/Cargo.toml",
        "desktop/crates/runtime/build.rs",
        "src/kernelledger/worker.py",
    ]
    for directory in (
        "desktop/crates/runtime/src",
        "desktop/crates/protocol/src",
        "desktop/crates/daemon/src",
    ):
        paths.extend(
            p.relative_to(ROOT).as_posix() for p in (ROOT / directory).rglob("*") if p.is_file()
        )
    snapshot = hashlib.sha256()
    for path in sorted(paths):
        snapshot.update(path.encode() + b"\0")
        snapshot.update(hashlib.sha256((ROOT / path).read_text(encoding="utf-8").encode()).digest())
    return snapshot.hexdigest()


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def verify(path: Path) -> dict:
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)) or "manifest.json" not in names:
            raise ValueError("invalid or duplicate package entries")
        if archive.getinfo("manifest.json").file_size > 1048576:
            raise ValueError("oversized package manifest")
        manifest = json.loads(archive.read("manifest.json"))
        if manifest.get("schema") != "kernelledger.native-package/1":
            raise ValueError("unsupported package schema")
        files = manifest["files"]
        if len(set(files) & {"kernelledger-daemon", "kernelledger-daemon.exe"}) != 1:
            raise ValueError("package must contain exactly one daemon executable")
        if set(names) != {"manifest.json", *files}:
            raise ValueError("unexpected package entries")
        for name, expected in files.items():
            if name not in (
                "kernelledger-daemon",
                "kernelledger-daemon.exe",
                "README.txt",
                "licenses.json",
            ):
                raise ValueError("invalid package path")
            item = archive.getinfo(name)
            if item.file_size > (
                512 * 1024 * 1024 if name.startswith("kernelledger-daemon") else 16 * 1024 * 1024
            ):
                raise ValueError(f"oversized package entry: {name}")
            if item.file_size != expected["byte_size"]:
                raise ValueError(f"size mismatch: {name}")
            if digest(archive.read(name)) != expected["sha256"]:
                raise ValueError(f"hash mismatch: {name}")
        return manifest


def package(binary: Path, output: Path, cargo: str, git: str = "git") -> dict:
    diagnostics = json.loads(subprocess.check_output([str(binary.resolve()), "diagnostics"]))
    snapshot = source_snapshot()
    if diagnostics.get("build_source_sha256") != snapshot:
        raise ValueError(
            "binary does not match current native source snapshot; rebuild before packaging"
        )
    resource = diagnostics["worker_resource"]
    if resource["sha256"] != digest((ROOT / "src/kernelledger/worker.py").read_bytes()):
        raise ValueError("binary worker resource differs from source; rebuild the daemon")
    license_lines = subprocess.check_output(
        [
            cargo,
            "tree",
            "--manifest-path",
            str(ROOT / "desktop/Cargo.toml"),
            "--locked",
            "-p",
            "kernelledger-daemon",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--format",
            "{p}|{l}",
        ],
        text=True,
    ).splitlines()
    licenses = []
    for line in sorted(set(license_lines)):
        dependency, expression = line.split("|", 1)
        expression = expression.removesuffix(" (*)")
        if not expression:
            raise ValueError(f"missing license metadata: {dependency}")
        licenses.append(
            {"dependency": dependency.split(" (", 1)[0], "license_expression": expression}
        )
    name = "kernelledger-daemon.exe" if platform.system() == "Windows" else "kernelledger-daemon"
    readme = (
        b"KernelLedger experimental native runtime\n\n"
        b"This archive is for explicit local opt-in evaluation, not a qualified release.\n"
        b"Install a supported project CPython interpreter and KernelLedger package first.\n"
        b"The worker is embedded. Windows token ACL setup retains the installed Python utility.\n"
        b"Start: kernelledger-daemon serve-experimental --project ABSOLUTE_PATH --python ABSOLUTE_PYTHON\n"
        b"Use existing kl --project PATH run/status/interrupt/checkpoint/restore/stop commands.\n"
        b"Interactive CLI run relays prompts and replies; disconnect releases its claim for desktop handoff.\n"
        b"Opt in at startup with KERNELLEDGER_RUNTIME=native and an absolute KERNELLEDGER_DAEMON path.\n"
        b"The Rust desktop can also discover a bundled sibling daemon. An existing owner is reused.\n"
        b"Desktop protocol 1 supports durable operations, frozen batches, verified blob ranges, and input claims.\n"
        b"Secret input is redacted unless record_secret_input is enabled in the project config at startup.\n"
        b"Committed change and execution feeds use bounded pages, resync signals, and subscriber write deadlines.\n"
        b"Training mutation and sharing mutations remain unavailable.\n"
        b"Native durable request hashing refuses unqualified numeric forms before admission.\n"
        b"Pipe output uses bounded UTF-8 capture; late chunks become unattributed session history.\n"
        b"Background-output/process-tree stress qualification remains open. No native GPU snapshots.\n"
        b"Legacy idempotency requires both client_id and client_request_id; retain the same key on retry.\n"
        b"Inspect the operation before retrying any unknown outcome.\n\n"
        b"Rollback: stop the native runtime with kl --project PATH stop. Wait for supervisor.json\n"
        b"to disappear, set KERNELLEDGER_RUNTIME=python, then run the CLI to start the Python supervisor.\n"
        b"This ends the live namespace; checkpoint first when possible. Source and history formats stay compatible.\n"
        b"Set KERNELLEDGER_HELPER=python (or unset it) to restore the Python desktop helper.\n"
        b"Do not switch implementations while another owner holds the notebook lock.\n\n"
        b"licenses.json inventories Cargo license expressions, not the complete license-text release gate.\n"
    )
    payloads = {
        name: binary.read_bytes(),
        "README.txt": readme,
        "licenses.json": (json.dumps(licenses, indent=2) + "\n").encode(),
    }
    manifest = {
        "schema": "kernelledger.native-package/1",
        "experimental": True,
        "platform": platform.system(),
        "architecture": platform.machine(),
        "git_commit": subprocess.check_output(
            [git, "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "source_snapshot_sha256": snapshot,
        "dirty_tree": bool(subprocess.check_output([git, "status", "--porcelain"], cwd=ROOT)),
        "worker": resource,
        "files": {
            name: {"sha256": digest(data), "byte_size": len(data)}
            for name, data in payloads.items()
        },
    }
    payloads["manifest.json"] = (json.dumps(manifest, indent=2) + "\n").encode()
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(payloads.items()):
            item = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            item.compress_type = zipfile.ZIP_DEFLATED
            item.external_attr = (
                0o100755 if name.startswith("kernelledger-daemon") else 0o100644
            ) << 16
            archive.writestr(item, data)
    return verify(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--verify", type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--git", default="git")
    args = parser.parse_args()
    if args.verify:
        result = verify(args.verify)
    else:
        if args.binary is None or args.out is None:
            parser.error("--binary and --out are required for packaging")
        result = package(args.binary, args.out, args.cargo, args.git)
    print(json.dumps(result, indent=2))
