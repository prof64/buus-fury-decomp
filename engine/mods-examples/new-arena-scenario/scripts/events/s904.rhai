// Event 904 (mod: new-arena-scenario). The champion: prize, flag, back to the forest.
fn event() {
    [
        #{ node: 0, say: "Unbelievable! We have a new champion! Your prize: two Senzu Beans!", box: 10, next: 1 },
        #{ node: 1, code: "n1_", entry: "main", next: 2 },
        #{ node: 2, say: "Heh, that was a good workout!", box: 55, next: 3 },
        #{ node: 3, code: "n3_", entry: "main", next: 4 },
        #{ node: 4, end: 0 },
    ]
}

// node 1: the prize (supply 21 = Senzu Bean; give_item shows the item-get pose) and the champion flag
fn n1_main() {
    give_item(21);
    add_item(21, 1);
    set_flag(1100);
}

// node 3: back to the announcer
fn n3_main() {
    warp_fade(8, 3, 1, 488, 496);
}
