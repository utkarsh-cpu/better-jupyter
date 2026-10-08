import argparse
import ctypes
import json
import time
from pathlib import Path
from ctypes import wintypes
from PIL import ImageGrab

u = ctypes.windll.user32
u.SetProcessDPIAware()
p = argparse.ArgumentParser()
p.add_argument('action', choices=['windows', 'capture', 'click', 'key', 'text', 'resize', 'scroll', 'release'])
p.add_argument('args', nargs='*')
a = p.parse_args()
windows = []
@ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
def enum(h, _):
    if u.IsWindowVisible(h):
        b = ctypes.create_unicode_buffer(512)
        u.GetWindowTextW(h, b, 512)
        pid = wintypes.DWORD()
        u.GetWindowThreadProcessId(h, ctypes.byref(pid))
        app_pid = int(Path(__file__).with_name('app.pid').read_text(encoding='utf-8-sig').strip())
        if b.value or pid.value == app_pid:
            r = wintypes.RECT()
            u.GetWindowRect(h, ctypes.byref(r))
            windows.append({'handle': h, 'pid': pid.value, 'title': b.value, 'rect': [r.left,r.top,r.right,r.bottom]})
    return True
u.EnumWindows(enum, 0)
if a.action == 'windows':
    print(json.dumps(windows))
    raise SystemExit()
w = next((w for w in windows if w['pid'] == int(Path(__file__).with_name('app.pid').read_text(encoding='utf-8-sig').strip())), None)
if not w:
    raise SystemExit('KernelLedger window not found')
h = w['handle']
if a.action == 'release':
    u.SetWindowPos(h, -2, 0, 0, 0, 0, 0x0003)
    raise SystemExit()
u.ShowWindow(h, 9)
fg = u.GetForegroundWindow()
tid = u.GetWindowThreadProcessId(fg, None)
own_tid = ctypes.windll.kernel32.GetCurrentThreadId()
u.AttachThreadInput(own_tid, tid, True)
u.SetWindowPos(h, -1, 0, 0, 0, 0, 0x0003)
u.SetForegroundWindow(h)
u.BringWindowToTop(h)
u.AttachThreadInput(own_tid, tid, False)
time.sleep(.2)
active_pid = wintypes.DWORD()
u.GetWindowThreadProcessId(u.GetForegroundWindow(), ctypes.byref(active_pid))
if a.action in ('click', 'key', 'text', 'scroll') and active_pid.value != w['pid']:
    raise SystemExit('Input refused: KernelLedger does not own foreground focus')
x0, y0, x1, y1 = w['rect']
if a.action == 'capture':
    time.sleep(.4)
    path = Path(a.args[0]).resolve()
    shot = ImageGrab.grab(bbox=(x0,y0,x1,y1), all_screens=True)
    shot.save(path)
    shot.resize((shot.width//2, shot.height//2)).save(path.with_stem(path.stem+'-view'))
    print(json.dumps({'path':str(path), 'window':w}))
elif a.action == 'click':
    u.SetCursorPos(x0+int(a.args[0])*2, y0+int(a.args[1])*2)
    u.mouse_event(2,0,0,0,0)
    u.mouse_event(4,0,0,0,0)
elif a.action == 'key':
    keys = {'ctrl':17,'shift':16,'alt':18,'enter':13,'esc':27,'tab':9,'down':40,'up':38,'home':36,'end':35,'delete':46,'f4':115}
    codes = [keys[k.lower()] if k.lower() in keys else ord(k.upper()) for k in a.args]
    for k in codes: u.keybd_event(k,0,0,0)
    for k in reversed(codes): u.keybd_event(k,0,2,0)
elif a.action == 'text':
    class KI(ctypes.Structure):
        _fields_ = [('wVk',wintypes.WORD),('wScan',wintypes.WORD),('dwFlags',wintypes.DWORD),('time',wintypes.DWORD),('dwExtraInfo',ctypes.c_size_t)]
    class MI(ctypes.Structure):
        _fields_ = [('dx',wintypes.LONG),('dy',wintypes.LONG),('mouseData',wintypes.DWORD),('dwFlags',wintypes.DWORD),('time',wintypes.DWORD),('dwExtraInfo',ctypes.c_size_t)]
    class IU(ctypes.Union):
        _fields_ = [('ki',KI),('mi',MI)]
    class IN(ctypes.Structure):
        _fields_ = [('type',wintypes.DWORD),('data',IU)]
    for c in ' '.join(a.args):
        for flags in (4,6):
            v = IN(type=1, data=IU(ki=KI(0,ord(c),flags,0,0)))
            u.SendInput(1,ctypes.byref(v),ctypes.sizeof(v))
elif a.action == 'resize':
    u.MoveWindow(h,20,20,int(a.args[0]),int(a.args[1]),True)
elif a.action == 'scroll':
    u.SetCursorPos(x0+int(a.args[0])*2,y0+int(a.args[1])*2)
    u.mouse_event(0x0800,0,0,int(a.args[2]),0)
time.sleep(.3)
