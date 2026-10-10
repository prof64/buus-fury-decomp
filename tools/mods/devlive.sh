#!/bin/sh
# tools/mods/devlive.sh: --dev hot reload in the actual headless app loop (real time, muted): a temp copy of the story-tweaks
# example; once the app reports it is watching, a broken JSON edit (must be rejected, the run goes on), then a good
# edit (must reload and restart the scene); the app's stderr (with arrival times) is the evidence
R=$(cd "$(dirname "$0")/../.." && pwd)
BIN=$R/engine/target-side/release/buus-fury
CONTENT=$R/engine/target/tmp/wf-core-field-content-2
T=$(mktemp -d /tmp/devlive.XXXXXX)
cp -R "$R/engine/mods-examples/story-tweaks" "$T/st"
P="$T/st/characters/goku.patch.json"
cp "$P" "$T/orig.json"
"$BIN" --content "$CONTENT" --headless --mute --realtime --dev --mod "$T/st" --story 3-1 --frames 900 \
  --shot "$T/end.ppm" > "$T/app.log" 2>&1 &
pid=$!
t0=$(date +%s)
until grep -q "dev mode: watching" "$T/app.log"; do sleep 0.2; done
echo "watching after $(( $(date +%s) - t0 )) s"
sleep 2
echo '{ "new_game": { "STR": ' > "$P"
echo "broken edit written at $(( $(date +%s) - t0 )) s"
sleep 3
sed 's/"STR": *[0-9]*/"STR": 77/' "$T/orig.json" > "$P"
echo "good edit written at $(( $(date +%s) - t0 )) s"
wait $pid
echo "app exit $? after $(( $(date +%s) - t0 )) s"
echo "--- app log"
cat "$T/app.log" | grep -v "^ *$" | head -40
