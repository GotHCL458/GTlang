#!/usr/bin/env python3
"""GTLang OS 调试脚本：nasm 组装 -> 启动 qemu -> 捕获串口输出。"""
import os, subprocess, sys, tempfile, shutil

ROOT = os.path.dirname(os.path.abspath(__file__))
QEMU = os.path.join(os.path.dirname(ROOT), 'qemu', 'qemu-system-x86_64.exe')
NASM = r'C:\\Program Files\\NASM\\nasm.exe'

def assemble(asm, out):
    subprocess.check_call([NASM, '-f', 'bin', asm, '-o', out])

def build_image(dst):
    s1 = os.path.join(ROOT, 'build', 'stage1.bin')
    os.makedirs(os.path.dirname(s1), exist_ok=True)
    assemble(os.path.join(ROOT, 'boot', 'stage1.asm'), s1)
    with open(dst, 'wb') as f:
        f.write(open(s1, 'rb').read())
        f.write(b'\x00' * (1474560 - 512))   # 1.44MB 软盘镜像

def run(img, timeout=6):
    cmd = [QEMU, '-drive', 'format=raw,file=' + img, '-serial', 'stdio',
           '-display', 'none', '-no-reboot', '-m', '32']
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        return (p.stdout + p.stderr).decode('utf-8', 'replace')
    except subprocess.TimeoutExpired as e:
        out = (e.stdout or b'') + (e.stderr or b'')
        return out.decode('utf-8', 'replace')

def main():
    img = os.path.join(ROOT, 'build', 'os.img')
    os.makedirs(os.path.dirname(img), exist_ok=True)
    build_image(img)
    print('[run] booting', img)
    out = run(img)
    sys.stdout.write(out)
    print('[run] done')

if __name__ == '__main__':
    main()
