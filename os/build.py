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
    arch = sys.argv[1] if len(sys.argv) > 1 else os.environ.get('GTC_OS_ARCH', 'x86_64')
    sh(GTC, '--bare', '--target', arch, os.path.join(ROOT, 'kernel.gt'), '-o', os.path.join(B, 'kernel.o'))
    triple = 'x86_64-unknown-none-elf' if arch.startswith('x86_64') else 'i386-unknown-none-elf'
    sh(CLANG, '-target', triple, '-ffreestanding', '-nostdlib',
       '-fno-stack-protector', '-c', os.path.join(ROOT, 'boot', 'rt_bare.c'), '-o', os.path.join(B, 'rt_bare.o'))
    if arch.startswith('x86_64'):
        sh(CLANG, '-target', 'x86_64-unknown-none-elf', '-ffreestanding', '-nostdlib',
           '-c', os.path.join(ROOT, 'boot', 'irq.S'), '-o', os.path.join(B, 'irq.o'))
        sh(CLANG, '-target', 'x86_64-unknown-none-elf', '-ffreestanding', '-nostdlib',
           '-c', os.path.join(ROOT, 'boot', 'task.S'), '-o', os.path.join(B, 'task.o'))
    sh(LLD, '-T', os.path.join(ROOT, 'boot', 'kernel.ld'), '-o', os.path.join(B, 'kernel.elf'),
       os.path.join(B, 'kernel.o'), os.path.join(B, 'rt_bare.o'),
       *([os.path.join(B, 'irq.o'), os.path.join(B, 'task.o')] if arch.startswith('x86_64') else []))
    sh(OBJCOPY, '-O', 'binary', os.path.join(B, 'kernel.elf'), os.path.join(B, 'kernel.bin'))
    sh(NASM, '-f', 'bin', os.path.join(ROOT, 'boot', 'stage1.asm'), '-o', os.path.join(B, 'stage1.bin'))
    s2_name = 'stage2.asm' if arch.startswith('x86_64') else 'stage2_32.asm'
    sh(NASM, '-f', 'bin', os.path.join(ROOT, 'boot', s2_name), '-o', os.path.join(B, 'stage2.bin'))
    img = os.path.join(B, 'os.img')
    with open(img, 'wb') as f:
        s1 = open(os.path.join(B, 'stage1.bin'), 'rb').read()
        f.write(s1.ljust(512, b'\x00'))
        s2 = open(os.path.join(B, 'stage2.bin'), 'rb').read()
        f.write(s2.ljust(512 * 512, b'\x00'))   # 256KB
        k = open(os.path.join(B, 'kernel.bin'), 'rb').read()
        f.write(k)
        f.write(b'\x00' * (1474560 - 512 - 512 * 512 - len(k)))
    return img
def run(img, timeout=8):
    cmd = [QEMU, '-drive', 'if=ide,format=raw,file=' + img, '-serial', 'stdio',
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
