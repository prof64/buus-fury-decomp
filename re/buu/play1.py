from common import *
e = emu('buu_menus.ss')
for i in range(12):
    e.press(KEY.A, 4, 60)
    e.screenshot(WORK / f'p1_{i}.png', scale=1)
e.save_file(WORK / 'p1.ss')
