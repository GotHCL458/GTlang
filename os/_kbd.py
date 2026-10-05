import os, subprocess, time
ROOT = r'D:\gtc\gtc_rust\os'
QEMU = r'D:\gtc\gtc_rust\qemu\qemu-system-x86_64.exe'
img = os.path.join(ROOT, 'build', 'os.img')
# 用 QEMU monitor 发送按键
cmd = [QEMU, '-drive', 'format=raw,file=' + img, '-serial', 'stdio',
       '-display', 'none', '-no-reboot', '-m', '32',
       '-monitor', 'stdio']
# 分开：serial 用文件，monitor 用管道
outf = os.path.join(ROOT, 'build', 'serial.txt')
cmd = [QEMU, '-drive', 'format=raw,file=' + img, '-serial', 'file:' + outf,
       '-display', 'none', '-no-reboot', '-m', '32', '-monitor', 'none']
p = subprocess.Popen(cmd)
time.sleep(2)
# 用 QEMU 的 sendkey 需要 monitor；这里改用 -device 或直接退出
time.sleep(3)
p.terminate()
print(open(outf, encoding='utf-8', errors='replace').read())