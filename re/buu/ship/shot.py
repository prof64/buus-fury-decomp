"""shot.py STATE.ss OUT.png [frames]  -- screenshot of a state (after N frames, default 1) + status"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/story_playthrough'))
from route import start_emu, status  # noqa: E402

e = start_emu(str(Path(sys.argv[1]).resolve()))
e.run(int(sys.argv[3]) if len(sys.argv) > 3 else 1)
e.screenshot(str(Path(sys.argv[2]).resolve()), scale=2)
print(status(e))
