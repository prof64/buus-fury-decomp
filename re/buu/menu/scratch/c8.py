import json
R = '/Users/prof/source/buus-fury-decomp/'
p = R + 're/buu/menu/scenarios_title.py'
s = open(p).read()
a = """            if e.r32(o) == 0x08049898:
                meta['open']['cursor_t'] = e.r32(o + 8)
        meta['open']['sel'] = e.r32(m + 0x10C)"""
b = """            if e.r32(o) == 0x08049898:
                # the counter is one step ahead in the mode's next iteration (measured); at a multiple of 8 the
                # redraw it asked for is in progress: the previous picture (8 steps earlier) is on screen
                t = e.r32(o + 8) + 1
                meta['open']['cursor_t'] = t
                if t % 8 == 0:
                    meta['open']['prev'] = {'kind': 'title', 'page': page, 'cursor_t': t - 8, 'sel': e.r32(m + 0x10C)}
        meta['open']['sel'] = e.r32(m + 0x10C)"""
if a in s:
    s = s.replace(a, b)
    open(p, 'w').write(s)
for n in ('title_menus', 'title_newgame', 'title_continue', 'title_delete'):
    q = R + 'data/menu/%s.json' % n
    d = json.load(open(q))
    o = d['meta']['open']
    o['cursor_t'] = 120
    o['prev'] = {'kind': 'title', 'page': 'title', 'cursor_t': 112, 'sel': o.get('sel', 0)}
    json.dump(d, open(q, 'w'))
print('ok')
