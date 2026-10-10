"""palram.py STATEFILE OUT : dump palette RAM (1 KiB)"""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
e = emu(sys.argv[1])
open(sys.argv[2], 'wb').write(e.palram())
