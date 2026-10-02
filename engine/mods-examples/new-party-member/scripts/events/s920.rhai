// Event 920 (mod: new-party-member). Tien in the Southwest Forest (8-3) joins the party (character 10) and may
// take over right away. box 135 = portrait 127 (Tien); box = 8 + portrait index.
fn event() {
    [
        #{ node: 0, code: "n0_", entry: "main", next: 1 },
        #{ node: 1, say: "Babidi's men are everywhere. Three eyes see more than two - let me fight at your side.", box: 135, next: 2 },
        #{ node: 2, code: "n2_", entry: "main", next: 3 },
        #{ node: 3, choice: "Fight as Tien now?\n-Yes\n-No", box: 135, next: 4 },
        #{ node: 4, code: "n4_", entry: "main", next: 6 },
        #{ node: 5, say: "Call on me whenever you need me.", box: 135, next: 6 },
        #{ node: 6, end: 0 },
    ]
}

// already in the party: straight to the question
fn n0_main() {
    if in_party(10) != 0 {
        return 3;
    }
}

// join: the party roster bit of character 10 (the character menu lists him from now on)
fn n2_main() {
    join_party(10);
    set_flag(1110);
    play_sfx(45);
}

// choice 0 = Yes: the leader becomes Tien (his record, form 56, skills, equipment)
fn n4_main() {
    if get_menu_result() == 0 {
        set_character(10);
    } else {
        return 5;
    }
}
