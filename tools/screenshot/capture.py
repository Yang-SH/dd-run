"""dd-run GUI 自检截图工具（开发期临时用，不进产物）。

用法：python tools/screenshot/capture.py [输出路径]

流程：
1. 找到 dd-gui.exe 进程的主窗口（窗口初始在屏幕外 OFFSCREEN_*）
2. 用 SetWindowPos 把它搬到可见位置（200, 150）并置顶
3. 等一帧后抓取屏幕，裁出面板矩形存 PNG

注意：dd-gui 窗口默认以 hidden 创建（避免启动黑框），egui 不会给 hidden
窗口绘制，直接截到的是黑/空 backing buffer。调用方需让进程以
`DD_RUN_FORCE_VISIBLE=1` 启动（临时自检钩子），窗口才会真正 visible 并渲染。

依赖：`pip install mss psutil`（仅本机，不进项目依赖）。
"""
import sys
import time
import ctypes
from ctypes import wintypes

import mss
import psutil

APP_W, APP_H = 560, 460
WIN_X, WIN_Y = 200, 150

SW_SHOW = 5
SWP_SHOWWINDOW = 0x40
SWP_FRAMECHANGED = 0x20

user32 = ctypes.windll.user32

EnumWindows = user32.EnumWindows
EnumWindowsProc = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_int, ctypes.c_int)
GetWindowThreadProcessId = user32.GetWindowThreadProcessId
GetWindowTextW = user32.GetWindowTextW
GetWindowTextLengthW = user32.GetWindowTextLengthW
GetWindowRect = user32.GetWindowRect
SetWindowPos = user32.SetWindowPos
ShowWindow = user32.ShowWindow


def find_dd_gui_windows():
    """返回 [(hwnd, title, rect)] —— dd-gui.exe 进程的所有顶层窗口。"""
    out = []
    for proc in psutil.process_iter(["name", "pid"]):
        if proc.info["name"] != "dd-gui.exe":
            continue
        target_pid = proc.info["pid"]

        def callback(hwnd, _lparam, _pid=target_pid):
            pid = wintypes.DWORD()
            GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == _pid:
                length = GetWindowTextLengthW(hwnd)
                buff = ctypes.create_unicode_buffer(max(length + 1, 64))
                GetWindowTextW(hwnd, buff, length + 1)
                rect = wintypes.RECT()
                GetWindowRect(hwnd, ctypes.byref(rect))
                out.append((hwnd, buff.value, rect))
            return True

        EnumWindows(EnumWindowsProc(callback), 0)
    return out


def main() -> int:
    target = sys.argv[1] if len(sys.argv) > 1 else "D:/AI/project/dd-run/shot.png"

    wins = find_dd_gui_windows()
    if not wins:
        print("没有找到 dd-gui.exe 的窗口（进程未启动？）")
        return 1

    for hwnd, title, rect in wins:
        print(f"hwnd=0x{hwnd:x} title={title!r} 当前位置=({rect.left},{rect.top}) "
              f"尺寸={rect.right - rect.left}x{rect.bottom - rect.top}")
        ShowWindow(hwnd, SW_SHOW)
        SetWindowPos(hwnd, 0, WIN_X, WIN_Y, APP_W, APP_H,
                     SWP_SHOWWINDOW | SWP_FRAMECHANGED)

    time.sleep(1.2)  # 给 egui 至少一帧重绘时间

    with mss.MSS() as sct:
        box = {"left": WIN_X, "top": WIN_Y, "width": APP_W, "height": APP_H}
        img = sct.grab(box)
        mss.tools.to_png(img.rgb, img.size, output=target)

    print(f"已截图 -> {target}  ({APP_W}x{APP_H})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
