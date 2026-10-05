import os, subprocess, time
ROOT = r'D:\gtc\gtc_rust\os'
QEMU = r'D:\gtc\gtc_rust\qemu\qemu-system-x86_64.exe'
img = os.path.join(ROOT, 'build', 'kbd.img')
outf = os.path.join(ROOT, 'build', 'kbd_serial.txt')
# 用 -serial file 输出 + monitor tcp，但加 -device atkbd 显式键盘
cmd = [QEMU, '-drive', 'format=raw,file=' + img,
       '-serial', 'file:' + outf,
       '-display', 'none', '-no-reboot', '-m', '32',
       '-monitor', 'tcp:127.0.0.1:4456,server,nowait']
p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(2)
import socket
s = socket.create_connection(('127.0.0.1', 4456), timeout=5)
time.sleep(0.5)
# 先查 info mice / info qtree 看键盘设备
s.sendall(b'info qtree\n')
time.sleep(0.5)
resp = s.recv(65536).decode('utf-8', 'replace')
print('has kbd:', 'keyboard' in resp.lower() or 'kbd' in resp.lower())
for k in ['a', 'b']:
    s.sendall(('sendkey ' + k + chr(10)).encode())
    time.sleep(0.8)
s.close()
time.sleep(2)
p.terminate()
time.sleep(0.5)
if os.path.exists(outf): print(open(outf, encoding='utf-8', errors='replace').read())