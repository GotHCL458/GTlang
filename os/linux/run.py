#!/usr/bin/env python3
"""构建并运行 GTLang 伪 Linux 镜像。"""
import os, subprocess, sys

ROOT = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(os.path.dirname(ROOT))
QEMU = os.path.join(PROJ, 'qemu', 'qemu-system-x86_64.exe')
GTC = os.path.join(PROJ, 'target', 'release', 'gtc.exe')
MAIN = os.path.join(ROOT, 'kernel', 'main.gt')
IMG = os.path.join(ROOT, 'build', 'linux.img')

def build():
    os.makedirs(os.path.join(ROOT, 'build'), exist_ok=True)
    r = subprocess.run([GTC, '--os', MAIN, '-o', IMG], capture_output=True, cwd=PROJ)
    out = (r.stdout + r.stderr).decode('utf-8', 'replace')
    if r.returncode != 0:
        print(out)
        sys.exit(1)
    print('[linux] image:', IMG)

def run():
    # 优先 -nographic（纯串口，键盘=终端；彩色走 ANSI）
    cmd = [QEMU, '-drive', 'format=raw,file=' + IMG, '-serial', 'mon:stdio', '-nographic', '-m', '32']
    print('[linux] launching QEMU (type in this terminal; exit to quit)...')
    p = subprocess.Popen(cmd)
    try:
        p.wait()
    except KeyboardInterrupt:
        p.kill()

if __name__ == '__main__':
    build()
    run()
    print('[linux] done')
