"""script_v2_trace.py -- ground-truth traces for equipment effect scripts and a skill-learning story event.

equip_effect.json: Snake Way start; Goku's four equipment slots are set to items with effect scripts (16, 60, 101,
  140; equipment table 0x086F82EC +0x28), then the game's own stat recompute 0x08013C66(record) is called
  (injected call; it runs each item's effect script through the interpreter, 0x08013C5E).
skill_upgrade.json: event 102 (Pikkon defeat: Goku learns Super Saiyan = learn_skill(0, 20)) run through the
  game's event runner 0x08012588 (injected call, then natural; A mashed through the dialog)."""
from common import *
from script_trace import Tracer, traced_call

REC = 0x03001560


def equip():
    e = emu('buu_snakeway_start.ss')
    e.run(3)
    for slot, item in enumerate((16, 60, 101, 140)):
        e.w32(REC + 0x3C + 4 * slot, item)
    tr = Tracer(e)
    r, n = traced_call(tr, 0x08013C66, (REC, 0, 0, 0))
    tr.run(5)
    print(tr.save('equip_effect', {'state': 'buu_snakeway_start.ss',
                                   'how': 'Goku slots = equipment 16/60/101/140 (poked), injected call of the stat '
                                          'recompute 0x08013C66(record) which runs the effect scripts naturally'}),
          tr.summary())


def skill():
    e = emu('buu_snakeway_start.ss')
    e.run(3)
    tr = Tracer(e)
    before = list(e.read(REC + 0xC, 8))
    r, n = traced_call(tr, 0x08012588, (u32(0x0879B704 + 4 * 102), 0, 0, 0),
                       keyfn=lambda f: KEY.A if f % 20 < 3 else 0, max_frames=3000)
    after = list(e.read(REC + 0xC, 8))
    print('skills', before, '->', after, 'frames', n)
    print(tr.save('skill_upgrade', {'state': 'buu_snakeway_start.ss',
                                    'how': f'event 102 (Pikkon defeat) run via injected run_event 0x08012588, A mashed; '
                                           f'Goku skill list (count, ids) {before} -> {after}'}), tr.summary())


if __name__ == '__main__':
    equip()
    skill()
