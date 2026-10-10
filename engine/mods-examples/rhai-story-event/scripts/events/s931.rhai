// Event 931 (mod: rhai-story-event). The soldier's on_defeat callback: the reward.
fn event() {
    [
        #{ node: 0, say: "You did it! As promised - the Dodon Ray is yours. And take this, too.", box: 19, next: 1 },
        #{ node: 1, code: "n1_", entry: "main", next: 2 },
        #{ node: 2, say: "Learned the Dodon Ray! (L picks it, B fires.)", box: 0, next: 3 },
        #{ node: 3, end: 0 },
    ]
}

// node 1: the flag, the move (skill 27, skills/27_dodon_ray.json) for whoever leads, a Senzu Bean (supply 21)
fn n1_main() {
    set_flag(1131);
    learn_skill(get_character(), 27);
    give_item(21);
}
