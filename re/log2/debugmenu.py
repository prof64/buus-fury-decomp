from common import *
e=new('log2_title.ss')
e.w8(0x202b324+8,2)
e.press(KEY.A,3,60)
e.run(60)
e.screenshot(OUT+'dbg.png')
e.save_file(ST+'log2_debugmenu.ss')
