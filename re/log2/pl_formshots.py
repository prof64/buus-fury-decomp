"""Screenshot every form (0..21) for identification -> work/log2/pl_forms.png"""
from pl_warp import warp_char
from pl_common import *
from PIL import Image, ImageDraw
CH=[0]*6+[1]*2+[2]*4+[3]*7+[4]*2+[5]
tiles=[]
for f in range(22):
    e=warp_char(12,0,CH[f],f,240)
    for i in range(60): e.press(KEY.B,2,10)
    e.run(60)
    P=player(e); cam_x=e.r32(0x30010a4); cam_y=e.r32(0x30010a8)
    im=Image.frombytes('RGB',(240,160),e.pixels())
    x=e.r32(P+0x158)//256; y=e.r32(P+0x15c)//256
    # crop around screen center-ish (player is usually near center)
    t=im.crop((80,40,160,120)).resize((160,160),Image.NEAREST)
    d=ImageDraw.Draw(t); d.text((2,2),f'form {f} ch{CH[f]}',fill=(255,255,0))
    tiles.append(t)
G=Image.new('RGB',(160*6,160*4))
for i,t in enumerate(tiles): G.paste(t,((i%6)*160,(i//6)*160))
G.save(OUT+'pl_forms.png')
