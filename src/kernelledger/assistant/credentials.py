"""CLI credentials in the OS store. Desktop uses keyring with the same target."""

import ctypes
import os
import shutil
import subprocess
from ctypes import wintypes


class Credential(ctypes.Structure):
    _fields_ = [
        ("Flags", wintypes.DWORD),
        ("Type", wintypes.DWORD),
        ("TargetName", wintypes.LPWSTR),
        ("Comment", wintypes.LPWSTR),
        ("LastWritten", wintypes.FILETIME),
        ("CredentialBlobSize", wintypes.DWORD),
        ("CredentialBlob", ctypes.POINTER(ctypes.c_ubyte)),
        ("Persist", wintypes.DWORD),
        ("AttributeCount", wintypes.DWORD),
        ("Attributes", ctypes.c_void_p),
        ("TargetAlias", wintypes.LPWSTR),
        ("UserName", wintypes.LPWSTR),
    ]


def credential(action: str, provider: str, value: str = "") -> str:
    if provider not in ("openai", "openai-compatible"):
        raise ValueError("unsupported credential provider")
    if os.name == "nt":
        api = ctypes.WinDLL("Advapi32", use_last_error=True)
        target = "KernelLedger/assistant/" + provider
        if action == "get":
            pointer = ctypes.POINTER(Credential)()
            api.CredReadW.argtypes = [
                wintypes.LPCWSTR,
                wintypes.DWORD,
                wintypes.DWORD,
                ctypes.POINTER(ctypes.POINTER(Credential)),
            ]
            if not api.CredReadW(target, 1, 0, ctypes.byref(pointer)):
                if ctypes.get_last_error() == 1168:
                    return ""
                raise OSError("Windows credential read failed")
            try:
                return ctypes.string_at(
                    pointer.contents.CredentialBlob, pointer.contents.CredentialBlobSize
                ).decode("utf-16-le")
            finally:
                api.CredFree.argtypes = [ctypes.c_void_p]
                api.CredFree(pointer)
        if action == "delete":
            api.CredDeleteW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD]
            if not api.CredDeleteW(target, 1, 0) and ctypes.get_last_error() != 1168:
                raise OSError("Windows credential deletion failed")
            return ""
        raw = value.encode("utf-16-le")
        buffer = (ctypes.c_ubyte * len(raw)).from_buffer_copy(raw)
        item = Credential(
            Type=1,
            TargetName=target,
            CredentialBlobSize=len(raw),
            CredentialBlob=buffer,
            Persist=2,
            UserName=provider,
        )
        api.CredWriteW.argtypes = [ctypes.POINTER(Credential), wintypes.DWORD]
        if not api.CredWriteW(ctypes.byref(item), 0):
            raise OSError("Windows credential write failed")
        return ""
    executable = shutil.which("secret-tool")
    if not executable:
        if action == "get":
            return ""
        raise ValueError("secret-tool is unavailable; use KL_AI_API_KEY in this CLI environment")
    operation = {"get": "lookup", "set": "store", "delete": "clear"}[action]
    args = [executable, operation]
    if action == "set":
        args += ["--label=KernelLedger assistant"]
    args += [
        "service",
        "KernelLedger",
        "username",
        provider,
        "target",
        "KernelLedger/assistant/" + provider,
    ]
    result = subprocess.run(
        args, input=value if action == "set" else None, capture_output=True, text=True, timeout=15
    )
    if action == "set" and result.returncode:
        raise ValueError("OS credential storage failed")
    return result.stdout.rstrip("\n") if action == "get" and result.returncode == 0 else ""


def from_environment(provider: str) -> str:
    return (
        os.environ.get("KL_AI_API_KEY")
        or (os.environ.get("OPENAI_API_KEY", "") if provider == "openai" else "")
        or credential("get", provider)
    )
