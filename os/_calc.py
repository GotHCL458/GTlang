a = open(r'D:\gtc\gtc_rust\os\build\t16f.bin','rb').read()
import struct
# 找 0f 8d
for i in range(len(a)-4):
    if a[i]==0x0f and a[i+1]==0x8d:
        rel = struct.unpack('<h', a[i+2:i+4])[0]
        print('jge at', hex(i+0x7C00), 'rel', rel, 'target', hex(i+0x7C00+4+rel))
        break
# 找 put(10): b0 0a ba f8 03 ee
for i in range(len(a)-5):
    if a[i:i+6] == bytes([0xb0,0x0a,0xba,0xf8,0x03,0xee]):
        print('put(10) at', hex(i+0x7C00))