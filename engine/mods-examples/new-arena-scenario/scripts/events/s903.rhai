// Event 903 (mod: new-arena-scenario). Round 2 is down (on-defeat of actor 1).
fn event() {
    [
        #{ node: 0, say: "Incredible! Final round: the Shadow Warrior!", box: 10, next: 1 },
        #{ node: 1, code: "n1_", entry: "main", next: 2 },
        #{ node: 2, end: 0 },
    ]
}

fn n1_main() {
    spawn_map_object(2);
}
