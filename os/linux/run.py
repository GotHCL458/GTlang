#!/usr/bin/env python3
"""构建并运行 GTLang 伪 Linux 镜像（QEMU）。"""
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

def run(timeout=8):
    cmd = [QEMU, '-drive', 'format=raw,file=' + IMG,
           '-serial', 'stdio', '-display', 'none', '-no-reboot', '-m', '32']
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        out = (p.stdout + p.stderr).decode('utf-8', 'replace')
    except subprocess.TimeoutExpired as e:
        out = ((e.stdout or b'') + (e.stderr or b'')).decode('utf-8', 'replace')
    for line in out.splitlines():
        if 'WARNING' in line or 'Automatically' in line or 'Specify' in line:
            continue
        print(line)

if __name__ == '__main__':
    build()
    run()
    print('[linux] done')
