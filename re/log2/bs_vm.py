"""Fuller disassembler for the LoG2 event-script VM (interpreter 0x080092e4) + cutscene sequences (0x0800b4ae).
Opcodes (handlers 0x083b5c00[op]; VERIFIED by reading each handler 0x08008f44..0x0800929b):
 00 b push u8 | 01 v push zigzag varint | 02 i call builtin i | 03 i store var i (setter 0x083b5f70[i]) | 04 && | 05 ||
 06 neg | 07 + | 08 - | 09 * | 0a / | 0b == | 0c != | 0d > | 0e >= | 0f < | 10 <= | 11 end | 12 n jmp +n | 13 n jz +n
 14 pop | 15 i var[i]=pop+1? (pre-inc, 0x08009131) | 16 i ++var (push) | 17 i var++ (push old) | 18 i --var | 19 i var-- |
 1a i (0x08009225) | 1b loop-push | 1c n loop (dec counter, jump back n while nonzero) | 1d n v1..vn push n varints | >=4a builtin op-0x4a.
Builtins are named in NAMES where identified (see bs_builtins.py catalog)."""
from en_lib import *
NAMES={0x00:'local0',0x01:'local1',0x02:'local2',0x03:'local3',0x0d:'player.HP',0x0e:'player.maxHP',0x0f:'player.EP',
 0x10:'player.maxEP',0x11:'rand',0x12:'chance%',0x13:'hasItem',0x14:'!hasItem',0x15:'itemCount',0x16:'charLevel',
 0x28:'waitFrames',0x2e:'playSfx',0x54:'spawnMapObject(listB idx)',0x57:'spawnPickup(item)',0x68:'replaceActorWithMapObject(actor,idx)',
 0x7f:'spawnListDObject(idx)',0x4a:'respawnConditionalObjects',0x8b:'giveExp(char,amount)',0x62:'setLevel?(0x0800ac11)',
 0x92:'setLocal0',0x93:'setLocal1',0x94:'setLocal2',0x95:'setLocal3'}
BIN={0x04:'&&',0x05:'||',0x07:'+',0x08:'-',0x09:'*',0x0a:'/',0x0b:'==',0x0c:'!=',0x0d:'>',0x0e:'>=',0x0f:'<',0x10:'<='}
def varint(b,p):
    v=0
    while True:
        c=b[p]; p+=1; v=(v<<7)+(c&0x7f)
        if not c&0x80: break
    return (-(v>>1) if v&1 else v>>1), p
def bname(i): return NAMES.get(i,f'b{i:#04x}')
def disasm(addr, maxlen=400):
    o=addr&0x1ffffff; b=ROM; p=o; out=[]
    while p<o+maxlen:
        op=b[p]; p+=1
        if op==0x11: out.append('end'); break
        if op==0x00: out.append(f'{b[p]}'); p+=1
        elif op==0x01: v,p=varint(b,p); out.append(f'{v}')
        elif op==0x02: out.append(bname(b[p])+'()'); p+=1
        elif op==0x03: out.append(f'store[{b[p]:#x}]'); p+=1
        elif op in BIN: out.append(BIN[op])
        elif op==0x06: out.append('neg')
        elif op==0x12: out.append(f'jmp+{b[p]}'); p+=1
        elif op==0x13: out.append(f'jz+{b[p]}'); p+=1
        elif op==0x14: out.append('pop')
        elif 0x15<=op<=0x1a: out.append(f'varop{op:#x}[{b[p]:#x}]'); p+=1
        elif op==0x1b: out.append('looppush')
        elif op==0x1c: out.append(f'loop-{b[p]}'); p+=1
        elif op==0x1d:
            n=b[p]; p+=1; vals=[]
            for _ in range(n):
                v,p=varint(b,p); vals.append(v)
            out.append('['+','.join(map(str,vals))+']')
        elif op>=0x4a and op<=0xec: out.append(bname(op-0x4a)+'()')
        else: out.append(f'?{op:#x}'); break
    return out
CUT_H=0x083ec848+4
def text_at(a):
    o=a&0x1ffffff; e=ROM.index(b'\0',o); return ROM[o:e].decode('latin1')
def cutscene(a, n=200):
    """Walk a sequence (array of u32): entry = step ptr (u16 next index, u8 cmd, ...) or a small value = goto index.
    Follows the next-index chain from entry 0 (each step visited once; gotos followed). cmd0 = VM script at +3,
    cmd1 = dialogue (+3 u8 portrait, text at +4), other cmds = actor/camera actions (not decoded)."""
    out=[]; seen=set(); i=0; branches=[]
    while i is not None and i not in seen and len(out)<n:
        seen.add(i)
        p=u32(a+4*i)
        if p==0: break
        if p>>24!=8:
            if p<0x10000: out.append(dict(i=i,goto=p)); i=p; continue
            break
        nxt=u16(p); cmd=u8(p+2)
        d=dict(i=i,addr=hex(p),next=nxt,cmd=cmd)
        if cmd==0: d['script']=' '.join(disasm(p+3))
        elif cmd==1:
            try: d['text']=text_at(p+4)[:160]
            except Exception: pass
        out.append(d)
        if cmd==0 and 'jz+' in d.get('script',''):
            branches.append(i+1)
        i=nxt if nxt else None
    return out
if __name__=='__main__':
    import sys
    for a in sys.argv[1:]:
        print('== cutscene',a)
        for s in cutscene(int(a,16)): print(s)
