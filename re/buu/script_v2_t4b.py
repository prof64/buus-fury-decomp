"""return_to_title: mash START/A after the call and watch the mode / screen."""
from script_v2_lib import *
from script_nat_b_lib import run_code

e = emu('buu_snakeway_start.ss')
e.run(3)
run_code(e, asm([('n', 0x8e)]), max_frames=10)
for k in range(40):
    e.set_keys(KEY.START if k % 4 == 0 else (KEY.A if k % 4 == 2 else 0))
    e.run(15)
    if k in (10, 20, 39):
        shot(e, f'title_mash_{k}')
e.set_keys(0)
print('mode', hex(e.r32(e.r32(0x03000F90))), 'nonblack px', sum(1 for b in e.pixels() if b > 8))
