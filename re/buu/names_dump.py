"""names_dump.py -- dump the whole UTF-16 string table to work/buu/strings_all.txt (id<TAB>text) for name audits."""
from common import WORK
from strtab import table, ustr

out = [f'{i}\t{ustr(p, 400)!r}' for i, p in enumerate(table())]
(WORK / 'strings_all.txt').write_text('\n'.join(out))
print(len(out))
