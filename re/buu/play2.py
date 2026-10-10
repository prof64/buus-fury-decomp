from common import *
e = emu(WORK / 'p1.ss')
S = Shots(e, 'grid')
for i in range(16):
    e.press(KEY.START, 4, 30)
    e.press(KEY.A, 4, 60)
    S.snap()
S.write()
e.save_file(WORK / 'p2.ss')
