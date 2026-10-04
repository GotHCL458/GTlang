import os, subprocess, sys
ROOT = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(ROOT)
QEMU = os.path.join(PROJ, 'qemu', 'qemu-system-x86_64.exe')
B = os.path.join(ROOT, 'build')
def run16(binpath, timeout=6):
    img = os.path.join(B, 't16.img')
    os.makedirs(B, exist_ok=True)
    with open(img, 'wb') as f:
        f.write(open(binpath, 'rb').read())
        f.write(b'\x00' * (1474560 - os.path.getsize(binpath)))
    cmd = [QEMU, '-drive', 'format=raw,file=' + img, '-serial', 'stdio',
           '-display', 'none', '-no-reboot', '-m', '16']
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        return (p.stdout + p.stderr).decode('utf-8', 'replace')
    except subprocess.TimeoutExpired as e:
        return ((e.stdout or b'') + (e.stderr or b'')).decode('utf-8', 'replace')
if __name__ == '__main__':
    sys.stdout.write(run16(sys.argv[1]))
    print('[run] done')
