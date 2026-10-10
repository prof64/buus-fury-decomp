// Event 902 (mod: new-arena-scenario). Round 1 is down (on-defeat of actor 0).
fn event() {
    [
        #{ node: 0, say: "What a start! Round two: the Super Majin Fighter!", box: 10, next: 1 },
        #{ node: 1, code: "n1_", entry: "main", next: 2 },
        #{ node: 2, end: 0 },
    ]
}

fn n1_main() {
    spawn_map_object(1);
}
