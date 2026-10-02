"""String helpers for the native decoding: pointer -> text, and pointer -> string-table id."""
from common import u32
from strtab import ustr, STRTAB

_rev = None


def string_id(p):
    global _rev
    if _rev is None:
        _rev = {}
        for i in range(1600):
            q = u32(STRTAB + 4 * i)
            _rev.setdefault(q, i)
    return _rev.get(p)


def ustr_at(p):
    if not (0x08000000 <= p < 0x08800000):
        return hex(p)
    return f'#{string_id(p)} {ustr(p, 80)!r}'
