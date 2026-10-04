#!/usr/bin/env python3
"""16 位镜像运行：GTLang --asm16gen 产物 -> QEMU 软盘引导 -> 串口输出。"""
import os, subprocess, sys
ROOT = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(ROOT)
QEMU = os.path.join(PROJ, 'qemu', 'qemu-system-i386.exe')
def run(binpath, timeout=5):
    img = os.path.join(ROOT, 'build', 'mbr.img')
    os.makedirs(os.path.dirname(img), exist_ok=True)
    data = open(binpath, 'rb').read()
    with open(img, 'wb') as f:
        f.write(data)
        f.write(b'\x00' * (1474560 - len(data)))
    cmd = [QEMU, '-fda', img, '-serial', 'stdio', '-display', 'none', '-no-reboot', '-m', '16', '-boot', 'a']
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        return (p.stdout + p.stderr).decode('utf-8', 'replace')
    except subprocess.TimeoutExpired as e:
        return ((e.stdout or b'') + (e.stderr or b'')).decode('utf-8', 'replace')
if __name__ == '__main__':
    out = run(sys.argv[1])
    # 去掉 QEMU 的 raw 格式告警
    for line in out.splitlines():
        if 'WARNING' in line or 'Automatically' in line or 'Specify' in line or not line.strip():
            continue
        print(line)
    print('[run] done')
