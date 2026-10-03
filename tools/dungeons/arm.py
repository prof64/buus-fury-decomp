"""arm.py ADDR [N]: ARM-mode disassembly of N instructions at a ROM address."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 're' / 'buu'))
from common import dis  # noqa: E402

print(dis(int(sys.argv[1], 16), int(sys.argv[2]) if len(sys.argv) > 2 else 40, thumb=False))
