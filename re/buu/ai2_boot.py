"""Import bootstrap: lets these scripts run either inside re/buu (normal) or from a sibling worktree,
by falling back to the main re-dashboard re/buu directory for the shared helper modules."""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
try:
    import common  # noqa: F401
except ImportError:
    sys.path.append('/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
    import common  # noqa: F401
