#!/usr/bin/env python3
import os, subprocess, sys
ROOT = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(os.path.dirname(ROOT))
GTC = os.path.join(PROJ, 'target', 'release', 'gtc.exe')
MAIN = os.path.join(ROOT, 'kernel', 'main.gt')
OUT = os.path.join(ROOT, 'build', 'linux.img')
os.makedirs(os.path.join(ROOT, 'build'), exist_ok=True)
r = subprocess.run([GTC, '--os', MAIN, '-o', OUT], capture_output=True)
print(r.stdout.decode('utf-8', 'replace'))
if r.returncode != 0:
    print(r.stderr.decode('utf-8', 'replace'))
    sys.exit(1)
print('[linux] image:', OUT)
