import os, subprocess
ROOT = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(ROOT)
QEMU = os.path.join(PROJ, 'qemu', 'qemu-system-i386.exe')
img = os.path.join(ROOT, 'build', 'mbr.img')
# 软盘引导（-fda），显式 raw
cmd = [QEMU, '-fda', img, '-serial', 'stdio', '-display', 'none', '-no-reboot', '-m', '16', '-boot', 'a']
try:
    p = subprocess.run(cmd, capture_output=True, timeout=5)
    print('rc:', p.returncode)
    print('OUT:', (p.stdout + p.stderr).decode('utf-8','replace')[:500])
except subprocess.TimeoutExpired as e:
    print('TIMEOUT OUT:', ((e.stdout or b'') + (e.stderr or b'')).decode('utf-8','replace')[:500])