a = open(r'D:\gtc\gtc_rust\os\t16.bin','rb').read()
print('bytes:', ' '.join(f'{x:02x}' for x in a))