import os, subprocess
ROOT = r'D:\gtc\gtc_rust\os'
QEMU = r'D:\gtc\gtc_rust\qemu\qemu-system-i386.exe'
data = open(os.path.join(ROOT,'build','t16h.bin'),'rb').read()
img = os.path.join(ROOT,'build','manual.img')
open(img,'wb').write(data + b'\x00'*(1474560-len(data)))
print('img size', os.path.getsize(img), 'data len', len(data))
cmd = [QEMU, '-fda', img, '-serial', 'stdio', '-display', 'none', '-no-reboot', '-m', '16', '-boot', 'a']
try:
    p = subprocess.run(cmd, capture_output=True, timeout=6)
    print('rc', p.returncode)
    print('OUT[' + (p.stdout+p.stderr).decode('utf-8','replace') + ']')
except subprocess.TimeoutExpired as e:
    print('TIMEOUT OUT[' + ((e.stdout or b'')+(e.stderr or b'')).decode('utf-8','replace') + ']')