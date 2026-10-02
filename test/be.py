#!/usr/bin/env python3
"""测试用假后端：start 起一个后台 http.server，stop 按 PID 杀掉。"""
import os, sys, time, signal, subprocess

port = sys.argv[1] if len(sys.argv) > 1 else "18081"
action = sys.argv[2] if len(sys.argv) > 2 else "start"
pidf = os.path.join(os.environ.get("TEMP", "/tmp"), f"lyco-be-{port}.pid")

if action == "start":
    if os.path.exists(pidf):
        try:
            os.kill(int(open(pidf).read()), 0)
            print("already running"); sys.exit(0)
        except OSError:
            pass
    flags = 0x00000008 if os.name == "nt" else 0  # DETACHED_PROCESS
    p = subprocess.Popen(
        [sys.executable, "-m", "http.server", port, "--bind", "127.0.0.1"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=flags,
    )
    open(pidf, "w").write(str(p.pid))
    time.sleep(0.8)
    print("started", p.pid)
else:
    if os.path.exists(pidf):
        pid = int(open(pidf).read())
        try:
            os.kill(pid, signal.SIGTERM)
        except OSError:
            pass
        os.remove(pidf)
        print("stopped", pid)
    else:
        print("no pid")
