@echo off
rem dd-run 扩展示例的 Windows 启动器 —— **备选方案**，不是推荐路径。
rem
rem ⚠️ 为什么是「备选」：本文件调用裸 `python`，**依赖解释器位于 PATH 上**。
rem    Windows 上这远非默认（Store 版 / 未勾选 Add-to-PATH / IDE 自带解释器都不在
rem    PATH），而且 dd-run 从资源管理器启动时继承的是**系统 PATH** —— 于是会出现
rem    「在终端里自检全绿、双击 dd-run 却启动不了」的假阳性（实测踩到过）。
rem
rem ✅ 推荐：跑 `python install.py`，它会把解释器与脚本的**绝对路径**写进清单，
rem    宿主直接 spawn、不经 shell，零 PATH 依赖。见同目录 README.md。
rem
rem 仅当确认 `where python` 有输出时，才适合用本启动器。
rem
rem 用法：清单里写 "entry": { "command": "${EXT_DIR}/dd-ext-pymin" }，
rem 宿主在 Windows 上会依次补 .exe / .cmd / .bat（manifest-schema §7 规则 8）。
setlocal
set "PY=python"
where py >nul 2>nul && set "PY=py -3"
%PY% "%~dp0dd_ext_pymin.py" %*
