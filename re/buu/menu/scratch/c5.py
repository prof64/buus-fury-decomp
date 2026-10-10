"""make work/states/buu_title_saved.ss: Snakeway, save into slot 1 (save_menu), soft reset, the title idle"""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/tools/story_playthrough')
from cap import *
from route import soft_reset
from scenarios_shop import inject
from menuhelp import setup_party, setup_zenie
e = emu('buu_snakeway_start.ss')
setup_party([1, 2])(e)
setup_zenie(1234)(e)
e.run(3)
inject([0, ('n', 0x8C)])(e)
e.run(400)
e.screenshot(str(OUT / 'saved_field.png'), scale=1)
soft_reset(e)
e.run(900)
e.save_file(ROOT / 'work' / 'states' / 'buu_title_saved.ss')
e.screenshot(str(OUT / 'title_saved_state.png'), scale=1)
print('ok')
