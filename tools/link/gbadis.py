import capstone, struct, sys
r=open('roms/buu.gba','rb').read()
md=capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
s=int(sys.argv[1],16); n=int(sys.argv[2],16)
for i in md.disasm(r[s-0x08000000:s-0x08000000+n], s):
    extra=''
    if i.mnemonic=='ldr' and 'pc' in i.op_str:
        off=int(i.op_str.split('#')[1].rstrip(']'),16)
        a=((i.address+4)&~3)+off
        extra='  ; =%#x'%struct.unpack_from('<I',r,a-0x08000000)[0]
    print(hex(i.address), i.mnemonic, i.op_str, extra)
