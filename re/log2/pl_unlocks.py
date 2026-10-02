"""Find event-script calls that join characters / unlock moves.

Script VM (interpreter 0x080092E4, opcode table 0x083B5C00): op 0x00 xx = push u8; op 0x01 varint = push
(zigzag: v>>1, odd = negative); op 0x1D n v1..vn = push n zigzag varints; op 0x02 xx = call builtin xx
(builtin table 0x083B5D28). Party builtins (handlers 0x0800A9B6..0x0800AA7C):
 0x4F setJoined(char)  0x5B clearJoined(char)  0x50 setFlag1(char)  0x51 clearFlag1(char)
 0x52 unlockTransform(char) (flag bit5)        0x53 learn(char, n): flags |= 4<<n
   n: 0 special melee, 1 slot0 move, 2 slot1 move, 3 slot2 (transform), 4 slot3.
Writes work/log2/pl_unlock_calls.json and prints a table with nearby dialogue."""
from common import *
import re, json
NAMES=['Gohan','Piccolo','Vegeta','Trunks','Goku','Hercule']
BUILT={0x4f:'setJoined',0x5b:'clearJoined',0x50:'setFlag1',0x51:'clearFlag1',0x52:'unlockTransform',0x53:'learn'}
NARGS={k:1 for k in BUILT}; NARGS[0x53]=2
SLOTNAMES={0:{0:'Super Kick',1:'Masenko Ball',2:'Kamehameha'},1:{0:'Whirlspin/Spin Punch',1:'Special Beam Cannon',2:'Scatter Shot'},
           2:{0:'Two-Handed Smash',1:'Big Bang',2:'Energy Punch'},3:{0:'Cross Slash',1:'Burning Attack',2:'Sword Blast'},
           4:{0:'Flurry Punch',1:'Kamehameha',2:'Spirit Bomb'}}
def zig(v): return -(v>>1) if v&1 else v>>1
def args_before(o, n):
    """Return list of n args pushed immediately before offset o (call), or None."""
    # form A: 1D n varints
    for back in range(2, 2+3*n+2):
        s=o-back
        if s<0 or ROM[s]!=0x1d: continue
        cnt=ROM[s+1]
        if cnt!=n: continue
        p=s+2; vals=[]
        for _ in range(cnt):
            v=0
            while True:
                c=ROM[p]; p+=1; v=(v<<7)+(c&0x7f)
                if not c&0x80: break
            vals.append(zig(v))
        if p==o: return vals
    # form B: 00 xx pushes
    vals=[]; p=o
    for _ in range(n):
        if p>=2 and ROM[p-2]==0x00: vals.insert(0,ROM[p-1]); p-=2
        else: return None
    return vals
def text_near(o):
    a=re.findall(rb'[\x20-\x7e]{12,}',ROM[o-300:o]); b=re.findall(rb'[\x20-\x7e]{12,}',ROM[o:o+300])
    return (a[-1].decode()[:70] if a else ''), (b[0].decode()[:70] if b else '')
def scan():
  res=[]
  for m in re.finditer(rb'\x02[\x4f\x50\x51\x52\x53\x5b]',ROM[0x3b6000:0x3f0000]):
      o=0x3b6000+m.start(); b=ROM[o+1]
      args=args_before(o,NARGS[b])
      if args is None or not all(0<=x<=5 for x in args[:1]): continue
      if b==0x53 and not (0<=args[1]<=4): continue
      before,after=text_near(o)
      d=dict(addr=hex(0x08000000+o),builtin=hex(b),fn=BUILT[b],char=NAMES[args[0]],args=args,text_before=before,text_after=after)
      if b==0x53:
          d['unlocks']={0:'special melee',1:'slot 0',2:'slot 1',3:'slot 2 (transform)',4:'slot 3'}[args[1]]
          d['move']=SLOTNAMES.get(args[0],{}).get(args[1])
      res.append(d)
  return res
if __name__=='__main__':
    res=scan()
    json.dump(res,open(OUT+'pl_unlock_calls.json','w'),indent=1)
    for d in res:
        if d['fn'] in ('learn','unlockTransform','setJoined'):
            print(d['addr'],d['fn'],d['char'],d.get('move') or '', '|', d['text_before'][-50:], '||', d['text_after'][:50])
