"""OS-owned locks, child process lifetime, and token permissions."""

import ctypes
import os
import signal
from pathlib import Path
from typing import Any, ClassVar

# DETACHED_PROCESS keeps python.exe from allocating a console host.
# CREATE_NEW_PROCESS_GROUP stays set so Windows job shutdown still owns the tree.
DETACHED_PROCESS = 0x00000008
CREATE_NEW_PROCESS_GROUP = 0x00000200
CREATE_NO_WINDOW = 0x08000000
HIDDEN_CONSOLE = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW


class DirectoryLock:
    def __init__(self, state: Any) -> None:
        self.path = Path(state) / "lock"
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.file = None

    def acquire(self) -> Any:
        self.file = self.path.open("a+b")
        if self.path.stat().st_size == 0:
            self.file.write(b" ")
            self.file.flush()
        self.file.seek(0)
        try:
            if os.name == "nt":
                import msvcrt

                msvcrt.locking(self.file.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(self.file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            self.file.close()
            self.file = None
            pid = "unknown"
            try:
                import json

                pid = json.loads((self.path.parent / "supervisor.json").read_text())["pid"]
            except (OSError, ValueError, KeyError):
                pass
            raise ValueError(f"notebook is locked by supervisor pid {pid}") from None
        return self

    def close(self) -> Any:
        if self.file is not None:
            if os.name == "nt":
                import msvcrt

                self.file.seek(0)
                msvcrt.locking(self.file.fileno(), msvcrt.LK_UNLCK, 1)
            self.file.close()
            self.file = None


class ProcessTree:
    def __init__(self, pid: Any) -> None:
        self.pid, self.job = pid, None
        if os.name != "nt":
            return
        from ctypes import wintypes as w

        class Basic(ctypes.Structure):
            _fields_: ClassVar[Any]
            _fields_ = [
                ("per_process", ctypes.c_int64),
                ("per_job", ctypes.c_int64),
                ("flags", w.DWORD),
                ("min_ws", ctypes.c_size_t),
                ("max_ws", ctypes.c_size_t),
                ("active", w.DWORD),
                ("affinity", ctypes.c_size_t),
                ("priority", w.DWORD),
                ("scheduling", w.DWORD),
            ]

        class IO(ctypes.Structure):
            _fields_: ClassVar[Any]
            _fields_ = [
                (n, ctypes.c_uint64)
                for n in (
                    "read_ops",
                    "write_ops",
                    "other_ops",
                    "read_bytes",
                    "write_bytes",
                    "other_bytes",
                )
            ]

        class Extended(ctypes.Structure):
            _fields_: ClassVar[Any]
            _fields_ = [
                ("basic", Basic),
                ("io", IO),
                ("process_mem", ctypes.c_size_t),
                ("job_mem", ctypes.c_size_t),
                ("peak_process", ctypes.c_size_t),
                ("peak_job", ctypes.c_size_t),
            ]

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)

        kernel.CreateJobObjectW.argtypes = [ctypes.c_void_p, w.LPCWSTR]
        kernel.CreateJobObjectW.restype = w.HANDLE

        kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
        kernel.OpenProcess.restype = w.HANDLE

        kernel.SetInformationJobObject.argtypes = [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD]

        kernel.AssignProcessToJobObject.argtypes = [w.HANDLE, w.HANDLE]

        kernel.CloseHandle.argtypes = [w.HANDLE]

        kernel.TerminateJobObject.argtypes = [w.HANDLE, w.UINT]
        job = kernel.CreateJobObjectW(None, None)
        limits = Extended()
        limits.basic.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        process = kernel.OpenProcess(0x0100 | 0x0001, False, pid)
        try:
            if (
                not job
                or not process
                or not kernel.SetInformationJobObject(
                    job, 9, ctypes.byref(limits), ctypes.sizeof(limits)
                )
                or not kernel.AssignProcessToJobObject(job, process)
            ):
                if job:
                    kernel.CloseHandle(job)
                raise OSError(ctypes.get_last_error(), "cannot contain worker in Job Object")
        finally:
            if process:
                kernel.CloseHandle(process)
        self.job, self.kernel = job, kernel

    def kill(self) -> Any:
        if self.job:
            self.kernel.TerminateJobObject(self.job, 1)
        elif os.name != "nt":
            try:
                os.killpg(self.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    def close(self) -> Any:
        self.kill()
        if self.job:
            self.kernel.CloseHandle(self.job)
            self.job = None


def secure_token(path: Any, verify: bool = False) -> bool:
    if os.name != "nt":
        if verify:
            return not bool(Path(path).stat().st_mode & 0o077)
        os.chmod(path, 0o600)
        return True
    from ctypes import wintypes as w

    advapi = ctypes.WinDLL("advapi32", use_last_error=True)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetCurrentProcess.restype = w.HANDLE

    kernel.CloseHandle.argtypes = [w.HANDLE]

    kernel.LocalFree.argtypes = [ctypes.c_void_p]

    advapi.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, ctypes.POINTER(w.HANDLE)]

    advapi.GetTokenInformation.argtypes = [
        w.HANDLE,
        ctypes.c_int,
        ctypes.c_void_p,
        w.DWORD,
        ctypes.POINTER(w.DWORD),
    ]

    advapi.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(w.LPWSTR)]

    advapi.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = [
        w.LPCWSTR,
        w.DWORD,
        ctypes.POINTER(ctypes.c_void_p),
        ctypes.POINTER(w.DWORD),
    ]

    token, size = w.HANDLE(), w.DWORD()
    sid_text, descriptor = w.LPWSTR(), ctypes.c_void_p()
    if not advapi.OpenProcessToken(kernel.GetCurrentProcess(), 8, ctypes.byref(token)):
        raise OSError("cannot read current user token")
    try:
        advapi.GetTokenInformation(token, 1, None, 0, ctypes.byref(size))
        buf = ctypes.create_string_buffer(size.value)
        if not advapi.GetTokenInformation(token, 1, buf, size, ctypes.byref(size)):
            raise OSError("cannot read token SID")
        sid = ctypes.cast(buf, ctypes.POINTER(ctypes.c_void_p))[0]
        if not advapi.ConvertSidToStringSidW(sid, ctypes.byref(sid_text)):
            raise OSError("cannot convert token SID")
        sddl = "D:P(A;;FA;;;" + str(sid_text.value) + ")"
        if not advapi.ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl, 1, ctypes.byref(descriptor), None
        ):
            raise OSError("cannot build token ACL")
        # FILE_ALL_ACCESS is what SDDL "FA" stores. GENERIC_ALL is the other
        # full-control mask Windows may persist for the same grant.
        full_control = {0x1F01FF, 0x10000000}

        class AceHeader(ctypes.Structure):
            _fields_ = (
                ("AceType", ctypes.c_ubyte),
                ("AceFlags", ctypes.c_ubyte),
                ("AceSize", ctypes.c_ushort),
            )

        def dacl_of(security: ctypes.c_void_p) -> ctypes.c_void_p:
            present, defaulted, acl = w.BOOL(), w.BOOL(), ctypes.c_void_p()
            advapi.GetSecurityDescriptorDacl.argtypes = [
                ctypes.c_void_p,
                ctypes.POINTER(w.BOOL),
                ctypes.POINTER(ctypes.c_void_p),
                ctypes.POINTER(w.BOOL),
            ]
            advapi.GetSecurityDescriptorDacl.restype = w.BOOL
            if not advapi.GetSecurityDescriptorDacl(
                security, ctypes.byref(present), ctypes.byref(acl), ctypes.byref(defaulted)
            ):
                raise OSError("cannot read token DACL")
            if not present.value or not acl.value:
                raise OSError("token DACL is missing")
            return acl

        def user_only(security: ctypes.c_void_p) -> bool:
            # SE_DACL_PRESENT | SE_DACL_PROTECTED. Inheritance must be broken
            # so a parent directory cannot add another reader.
            control, revision = ctypes.c_ushort(), w.DWORD()
            advapi.GetSecurityDescriptorControl.argtypes = [
                ctypes.c_void_p,
                ctypes.POINTER(ctypes.c_ushort),
                ctypes.POINTER(w.DWORD),
            ]
            advapi.GetSecurityDescriptorControl.restype = w.BOOL
            if not advapi.GetSecurityDescriptorControl(
                security, ctypes.byref(control), ctypes.byref(revision)
            ):
                raise OSError("cannot read token security control")
            if (control.value & 0x1004) != 0x1004:
                return False

            class AclSize(ctypes.Structure):
                _fields_ = (
                    ("AceCount", w.DWORD),
                    ("AclBytesInUse", w.DWORD),
                    ("AclBytesFree", w.DWORD),
                )

            info = AclSize()
            advapi.GetAclInformation.argtypes = [
                ctypes.c_void_p,
                ctypes.c_void_p,
                w.DWORD,
                ctypes.c_int,
            ]
            advapi.GetAclInformation.restype = w.BOOL
            acl = dacl_of(security)
            if not advapi.GetAclInformation(acl, ctypes.byref(info), ctypes.sizeof(info), 2):
                raise OSError("cannot read token ACL")
            if info.AceCount != 1:
                return False
            ace = ctypes.c_void_p()
            advapi.GetAce.argtypes = [ctypes.c_void_p, w.DWORD, ctypes.POINTER(ctypes.c_void_p)]
            advapi.GetAce.restype = w.BOOL
            if not advapi.GetAce(acl, 0, ctypes.byref(ace)) or not ace.value:
                raise OSError("cannot read token ACE")
            header = AceHeader.from_address(ace.value)
            mask = w.DWORD.from_address(ace.value + ctypes.sizeof(AceHeader)).value
            # ACCESS_ALLOWED_ACE_TYPE is 0. Any inherit flag means the entry
            # did not come from the protected DACL written above.
            if header.AceType != 0 or header.AceFlags != 0 or mask not in full_control:
                return False
            advapi.EqualSid.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
            advapi.EqualSid.restype = w.BOOL
            return bool(advapi.EqualSid(ace.value + 8, sid))

        if verify:
            advapi.GetFileSecurityW.argtypes = [
                w.LPCWSTR,
                w.DWORD,
                ctypes.c_void_p,
                w.DWORD,
                ctypes.POINTER(w.DWORD),
            ]
            required = w.DWORD()
            advapi.GetFileSecurityW(str(path), 4, None, 0, ctypes.byref(required))
            actual = ctypes.create_string_buffer(required.value)
            if not advapi.GetFileSecurityW(str(path), 4, actual, required, ctypes.byref(required)):
                raise OSError("cannot read token ACL")
            return user_only(ctypes.cast(actual, ctypes.c_void_p))
        # SetFileSecurity ignores PROTECTED_DACL_SECURITY_INFORMATION, so an
        # inherited parent ACE can survive. SetNamedSecurityInfo applies it.
        advapi.SetNamedSecurityInfoW.argtypes = [
            w.LPWSTR,
            w.DWORD,
            w.DWORD,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_void_p,
        ]
        advapi.SetNamedSecurityInfoW.restype = w.DWORD
        name = ctypes.create_unicode_buffer(str(path))
        status = advapi.SetNamedSecurityInfoW(
            name, 1, 4 | 0x80000000, None, None, dacl_of(descriptor), None
        )
        if status:
            raise OSError(status, "cannot restrict token ACL")
        return True
    finally:
        kernel.CloseHandle(token)
        if sid_text:
            kernel.LocalFree(sid_text)
        if descriptor:
            kernel.LocalFree(descriptor)
