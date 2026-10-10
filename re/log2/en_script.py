"""Minimal disassembler for the LoG2 event-script VM (interpreter 0x080092e4, opcode table 0x083b5c00,
builtin table 0x083b5d28 = opcode table + 0x4a*4). Used here to decode enemy on-death (drop) scripts."""
from en_lib import *
ITEMTBL=0x086ade24
def item_name(i):
    a=ITEMTBL+56*i; n=u16(a+4)
    return cstr(u32(0x080e2840+4*n))
BUILTIN={0x0d:'player.HP',0x0e:'player.maxHP',0x0f:'player.EP',0x10:'player.maxEP',0x11:'rand',0x12:'chance%',
         0x13:'hasItem',0x14:'!hasItem',0x16:'charLevel',0x57:'spawnPickup',0x58:'spawnPickup2(0x0800c718)',0x00:'local0',0x01:'local1',0x02:'local2',0x03:'local3'}
BIN={0x04:'&&',0x05:'||',0x07:'+',0x08:'-',0x09:'*',0x0a:'/',0x0b:'==',0x0c:'!=',0x0d:'>',0x0e:'>=',0x0f:'<',0x10:'<='}
def varint(b,p):
    v=0
    while True:
        c=b[p]; p+=1; v=(v<<7)+(c&0x7f)
        if not c&0x80: break
    return (-(v>>1) if v&1 else v>>1), p
def disasm(addr, maxlen=600):
    o=addr&0x1ffffff; b=ROM; p=o; out=[]
    while p<o+maxlen:
        op=b[p]; a=0x08000000+p; p+=1
        if op==0x11: out.append((a,'end',None)); break
        if op==0x00: out.append((a,'push',b[p])); p+=1
        elif op==0x01: v,p=varint(b,p); out.append((a,'push',v))
        elif op==0x02: out.append((a,'call',BUILTIN.get(b[p],hex(b[p])))); p+=1
        elif op==0x03: out.append((a,'store',f'local{b[p]}' if b[p]<4 else hex(b[p]))); p+=1
        elif op in BIN: out.append((a,'op',BIN[op]))
        elif op==0x06: out.append((a,'neg',None))
        elif op==0x12: out.append((a,'jmp',p+1+b[p]+0x08000000)); p+=1
        elif op==0x13: out.append((a,'jz',p+1+b[p]+0x08000000)); p+=1
        elif op==0x14: out.append((a,'pop',None))
        elif op>=0x4a: out.append((a,'call',BUILTIN.get(op-0x4a,hex(op-0x4a))))
        else: out.append((a,'op?',hex(op))); break
    return out
def drop_rules(addr):
    """Pattern-match generic drop scripts: r=rand(N); then blocks 'if (r<T [&& cond]) spawnPickup(item)'."""
    ins=disasm(addr); rules=[]; N=None
    for i,(a,k,v) in enumerate(ins):
        if k=='call' and v=='rand' and i>0 and ins[i-1][1]=='push': N=ins[i-1][2]
        if k=='call' and v=='spawnPickup' and i>0 and ins[i-1][1]=='push':
            item=ins[i-1][2]
            # walk back to the threshold compare of this block
            j=i-1; T=None; cond=[]
            while j>0 and ins[j][1]!='jz': j-=1
            seg=ins[:j]
            # find last 'call local0, push T, op <' before j
            for q in range(len(seg)-1,1,-1):
                if seg[q][1]=='op' and seg[q][2]=='<' and seg[q-1][1]=='push' and seg[q-2][1]=='call' and seg[q-2][2]=='local0':
                    T=seg[q-1][2]
                    c=[x[2] for x in seg[q+1:] if x[1]=='call']
                    cond=c; break
            cnames=[]
            if 'player.HP' in cond: cnames.append('player HP < max HP')
            if 'player.EP' in cond: cnames.append('player EP < max EP')
            rules.append(dict(roll_below=T, requires=cnames, item_id=item, item=item_name(item)))
    return dict(roll=f'r = rand({N})', rules=rules)
if __name__=='__main__':
    import sys
    for s in sys.argv[1:]:
        a=int(s,16)
        for x in disasm(a): print(hex(x[0]),x[1],x[2] if not isinstance(x[2],int) or x[1]!='jmp' else hex(x[2]))
        print(drop_rules(a))
