#!/bin/sh
# Is it still exact? The whole real run, power-on to the credits, no-lag mode; exit 1 on any new stop (check.py).
cd "$(dirname "$0")/../.." && exec .venv/bin/python tools/wholerun/check.py "$@"
