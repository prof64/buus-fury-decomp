"""Where are the form controller slots (IWRAM 0x03001B60..0x03001B8C) referenced from in ROM?"""
import struct
rom = open('/Users/prof/source/buus-fury-decomp/roms/buu.gba', 'rb').read()
for slot in range(0x03001B60, 0x03001B90, 4):
    hits = [0x08000000 + i for i in range(0, len(rom) - 3, 4) if struct.unpack_from('<I', rom, i)[0] == slot]
    print(hex(slot), [hex(h) for h in hits[:12]])
