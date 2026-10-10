"""Mini decoder for the LoG2 script VM (interpreter 0x080092e4, opcode table 0x083b5c00,
'02 xx' sub-op table 0x083b5d28 == main table entry 0x4a+xx). Only ops needed for drop scripts are named."""
from common import *
OPS={0x04:'and',0x05:'or',0x06:'neg',0x07:'add',0x08:'sub',0x09:'mul',0x0a:'div',
     0x0b:'==',0x0c:'!=',0x0d:'>',0x0e:'>=',0x0f:'<',0x10:'<=',0x14:'pop'}
SUB={0:'L0',1:'L1',2:'L2',3:'L3',0x0d:'HP',0x0e:'maxHP',0x0f:'EP',0x10:'maxEP',0x11:'rand',0x12:'chance%',
     0x57:'drop_item_at_entity'}
def decode(a, maxlen=400):
    out=[]; end=a+maxlen
    while a<end:
        op=u8(a); s=a; a+=1
        if op==0: out.append((s,'push',u8(a))); a+=1
        elif op==2: out.append((s,'call',SUB.get(u8(a),hex(u8(a))))); a+=1
        elif op==3: out.append((s,'store',u8(a))); a+=1
        elif op==0x12: out.append((s,'jmp',a+1+u8(a))); a+=1
        elif op==0x13: out.append((s,'jz',a+1+u8(a))); a+=1
        elif op==0x11: out.append((s,'end',None)); break
        elif op in OPS: out.append((s,OPS[op],None))
        else: out.append((s,'op%02x'%op,None)); break
    return out
def drop_table(a):
    """Interpret the standard drop pattern: L0=rand(N); chain of (L0 < T [&& cond]) -> drop item."""
    ins=decode(a)
    if len(ins)<3 or ins[0][1]!='push' or ins[1][2]!='rand': return None
    N=ins[0][2]; rows=[]; i=3
    while i<len(ins):
        if ins[i][1]=='call' and ins[i][2]=='L0' and ins[i+1][1]=='push' and ins[i+2][1]=='<':
            T=ins[i+1][2]; j=i+3; cond=[]
            # optional condition: X Y != and
            if ins[j][1]=='call' and ins[j+1][1]=='call' and ins[j+2][1] in ('!=','<','>'):
                cond=f'{ins[j][2]} {ins[j+2][1]} {ins[j+1][2]}'; j+=4
            # jz, push item, call drop
            if ins[j][1]=='jz' and ins[j+1][1]=='push' and ins[j+2][2]=='drop_item_at_entity':
                rows.append(dict(roll_lt=T,item=ins[j+1][2],cond=cond or None)); i=j+3; continue
        i+=1
    return dict(roll='rand(%d)'%N,rows=rows)
if __name__=='__main__':
    import sys
    a=int(sys.argv[1],16)
    for x in decode(a): print(hex(x[0]),x[1],x[2] if not isinstance(x[2],int) or x[2]<0x8000000 else hex(x[2]))
    print(drop_table(a))
