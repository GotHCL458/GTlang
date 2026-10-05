import os, subprocess
QEMU = r'D:\gtc\gtc_rust\qemu\qemu-system-x86_64.exe'
img = os.path.join(os.environ['TEMP'], 'gtc_boot.img')
cmd = [QEMU, '-drive', 'if=floppy,format=raw,file=' + img, '-serial', 'stdio', '-display', 'none', '-no-reboot', '-m', '32', '-boot', 'a']
try:
    p = subprocess.run(cmd, capture_output=True, timeout=6)
    print((p.stdout + p.stderr).decode('utf-8','replace'))
except subprocess.TimeoutExpired as e:
    print(((e.stdout or b'')+(e.stderr or b'')).decode('utf-8','replace'))