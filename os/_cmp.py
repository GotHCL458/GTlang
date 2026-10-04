import sys
a = open(r'D:\gtc\gtc_rust\os\test_asm16.bin','rb').read()
b = open(r'D:\gtc\gtc_rust\os\test_nasm.bin','rb').read()
print('ours :', ' '.join(f'{x:02x}' for x in a))
print('nasm :', ' '.join(f'{x:02x}' for x in b))
print('len  :', len(a), len(b))