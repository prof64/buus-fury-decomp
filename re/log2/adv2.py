from common import *
import sys
from PIL import Image
src=sys.argv[1]; n=int(sys.argv[2]); key=getattr(KEY,sys.argv[3]) if len(sys.argv)>3 else KEY.A
e=new(src)
ims=[]
for i in range(n):
    e.press(key, 3, 27)
    if i%(max(1,n//8))==0 or i==n-1: ims.append(Image.frombytes('RGB',(240,160),e.pixels()))
ims=ims[-8:]
G=Image.new('RGB',(240*4,160*2))
for k,im in enumerate(ims): G.paste(im,((k%4)*240,(k//4)*160))
G.save(OUT+'grid.png')
e.save_file(OUT+'adv.ss')
