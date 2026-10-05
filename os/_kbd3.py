import os, subprocess, time, socket
ROOT = r'D:\gtc\gtc_rust\os'
QEMU = r'D:\gtc\gtc_rust\qemu\qemu-system-x86_64.exe'
img = os.path.join(ROOT, 'build', 'kbd.img')
outf = os.path.join(ROOT, 'build', 'kbd_serial.txt')
cmd = [QEMU, '-drive', 'format=raw,file=' + img,
       '-serial', 'file:' + outf,
       '-display', 'none', '-no-reboot', '-m', '32',
       '-monitor', 'tcp:127.0.0.1:4455,server,nowait']
p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(2)
try:
    s = socket.create_connection(('127.0.0.1', 4455), timeout=5)
    time.sleep(0.5)
    for k in ['a', 'b', 'c']:
        s.sendall(('sendkey ' + k + chr(10)).encode())
        time.sleep(0.8)
    s.close()
except Exception as e:
    print('monitor err:', e)
time.sleep(2)
p.terminate()
time.sleep(0.5)
if os.path.exists(outf):
    print(open(outf, encoding='utf-8', errors='replace').read())