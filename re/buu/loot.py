"""Monte-Carlo the enemy loot script (#497 'enemy loot', called as run_script(497, level, forced_item))
using the Python VM, to get per-kill drop expectations for a given loot level."""
import random
import sys
from collections import Counter
from vm import VM


def simulate(level, forced=-1, n=20000, equipped=(), seed=1):
    rng = random.Random(seed)
    vm = VM(equipped=equipped, rng=rng)
    counts = Counter()
    zenie = 0
    ndrops = Counter()
    for _ in range(n):
        vm.drops = []
        vm.G = [0, 0, 0, 0]
        vm.P = [level, forced, 0, 0]
        vm.run(497)
        ndrops[len(vm.drops)] += 1
        for d in vm.drops:
            if d[0] == 'zenie':
                zenie += d[1]
                counts[('zenie', d[1])] += 1
            else:
                counts[d] += 1
    return dict(level=level, forced=forced, samples=n,
                avg_drops=sum(k * v for k, v in ndrops.items()) / n,
                avg_zenie=zenie / n,
                per_kill={f'{k[0]}:{k[1]}': round(v / n, 5) for k, v in sorted(counts.items(), key=lambda kv: -kv[1])})


if __name__ == '__main__':
    lv = int(sys.argv[1]) if len(sys.argv) > 1 else 107
    r = simulate(lv)
    print(r['avg_drops'], r['avg_zenie'])
    for k, v in list(r['per_kill'].items())[:25]:
        print(k, v)
