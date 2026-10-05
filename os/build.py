#!/usr/bin/env python3
import os, subprocess, sys
ROOT = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(ROOT)
QEMU = os.path.join(PROJ, 'qemu', 'qemu-system-x86_64.exe')
NASM = r'C:\\Program Files\\NASM\\nasm.exe'
LLVM = r'D:\\LLVM\\bin'
CLANG = os.path.join(LLVM, 'clang.exe')
LLD = os.path.join(LLVM, 'ld.lld.exe')
OBJCOPY = os.path.join(LLVM, 'llvm-objcopy.exe')
GTC = os.path.join(PROJ, 'target', 'release', 'gtc.exe')
B = os.path.join(ROOT, 'build')
def sh(*cmd):
    r = subprocess.run(list(cmd), capture_output=True)
    if r.returncode != 0:
        sys.stderr.write(r.stdout.decode('utf-8','replace'))
        sys.stderr.write(r.stderr.decode('utf-8','replace'))
        raise SystemExit('failed: ' + ' '.join(map(str, cmd)))
    return r.stdout.decode('utf-8', 'replace')
def build():
    os.makedirs(B, exist_ok=True)
    sh(GTC, '--bare', '--target', 'x86_64', os.path.join(ROOT, 'kernel.gt'), '-o', os.path.join(B, 'kernel.o'))
    sh(CLANG, '-target', 'x86_64-unknown-none-elf', '-ffreestanding', '-nostdlib',
       '-fno-stack-protector', '-c', os.path.join(ROOT, 'boot', 'rt_bare.c'), '-o', os.path.join(B, 'rt_bare.o'))
    sh(LLD, '-T', os.path.join(ROOT, 'boot', 'kernel.ld'), '-o', os.path.join(B, 'kernel.elf'),
       os.path.join(B, 'kernel.o'), os.path.join(B, 'rt_bare.o'))
    sh(OBJCOPY, '-O', 'binary', os.path.join(B, 'kernel.elf'), os.path.join(B, 'kernel.bin'))
    sh(NASM, '-f', 'bin', os.path.join(ROOT, 'boot', 'stage1.asm'), '-o', os.path.join(B, 'stage1.bin'))
    sh(NASM, '-f', 'bin', os.path.join(ROOT, 'boot', 'stage2.asm'), '-o', os.path.join(B, 'stage2.bin'))
    img = os.path.join(B, 'os.img')
    with open(img, 'wb') as f:
        s1 = open(os.path.join(B, 'stage1.bin'), 'rb').read()
        f.write(s1.ljust(512, b'\x00'))
        s2 = open(os.path.join(B, 'stage2.bin'), 'rb').read()
        f.write(s2.ljust(512 * 128, b'\x00'))   # 64KB
        k = open(os.path.join(B, 'kernel.bin'), 'rb').read()
        f.write(k)
        f.write(b'\x00' * (1474560 - 512 - 512 * 128 - len(k)))
    return img
def run(img, timeout=8):
    cmd = [QEMU, '-drive', 'format=raw,file=' + img, '-serial', 'stdio',
           '-display', 'none', '-no-reboot', '-m', '32']
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        return (p.stdout + p.stderr).decode('utf-8', 'replace')
    except subprocess.TimeoutExpired as e:
        return ((e.stdout or b'') + (e.stderr or b'')).decode('utf-8', 'replace')
if __name__ == '__main__':
    img = build()
    print('[build] image:', img)
    sys.stdout.write(run(img))
    print('[run] done')
