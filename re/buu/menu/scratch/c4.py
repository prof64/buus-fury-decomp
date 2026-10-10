"""make work/states/buu_title.ss: soft reset from buu_menus.ss, boot screens, the title idle (frame 900)"""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/tools/story_playthrough')
from cap import *
from route import start_emu
e = start_emu('reset')
e.run(900)
e.save_file(ROOT / 'work' / 'states' / 'buu_title.ss')
e.screenshot(str(OUT / 'title_state.png'), scale=1)
# and one at the end of the boot: frame 0 = reset (for the boot sequence scenario)
e = start_emu('reset')
e.save_file(ROOT / 'work' / 'states' / 'buu_reset.ss')
print('ok')
