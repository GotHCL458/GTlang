#!/usr/bin/env python3
import os, subprocess, sys
ROOT = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(ROOT)
QEMU = os.path.join(PROJ, 'qemu', 'qemu-system-x86_64.exe')
def run(binpath, timeout=6):
    img = os.path.join(ROOT, 'build', 'floppy.img')
    data = open(binpath, 'rb').read()
    with open(img, 'wb') as f:
        f.write(data)
        f.write(b'\x00' * (1474560 - len(data)))
    cmd = [QEMU, '-drive', 'if=ide,format=raw,file=' + img, '-serial', 'stdio',
           '-display', 'none', '-no-reboot', '-m', '32', '-boot', 'a']
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        return (p.stdout + p.stderr).decode('utf-8', 'replace')
    except subprocess.TimeoutExpired as e:
        return ((e.stdout or b'') + (e.stderr or b'')).decode('utf-8', 'replace')
if __name__ == '__main__':
    out = run(sys.argv[1])
    for line in out.splitlines():
        if 'WARNING' in line or 'Automatically' in line or 'Specify' in line:
            continue
        print(line)
    print('[run] done')
