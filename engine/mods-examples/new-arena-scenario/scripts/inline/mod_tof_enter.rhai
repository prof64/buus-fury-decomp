// Tournament of Fury (90-1): the area's on-enter script. It only starts the intro event; the rounds chain
// through the enemies' on-defeat events (world/areas/90-1.json): s901 -> round 1 -> s902 -> round 2 -> s903 ->
// round 3 -> s904 (prize, back to the forest).
fn main() {
    run_event(901);
}
