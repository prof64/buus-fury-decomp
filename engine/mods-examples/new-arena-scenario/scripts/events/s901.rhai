// Event 901 (mod: new-arena-scenario). Arena intro, run by the area's on-enter script (mod_tof_enter).
fn event() {
    [
        #{ node: 0, say: "Ladies and gentlemen, the Tournament of Fury! Round one: an Elite Majin Fighter!", box: 10, next: 1 },
        #{ node: 1, code: "n1_", entry: "main", next: 2 },
        #{ node: 2, end: 0 },
    ]
}

// node 1: place round 1 = the area's actor list entry 0 (world/areas/90-1.json objects[0])
fn n1_main() {
    spawn_map_object(0);
}
