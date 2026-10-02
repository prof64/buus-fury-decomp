// Hand translations of original Buu's Fury scripts in the "run-to-completion + action timeline" style.
// Raw native ids (op / op_now) where the native is not named yet. Integer-only (Rhai `no_float`).
// Every handler takes one int and returns an int.

// Script #450 (Janemba, actor 149, defeated -> Super Janemba, actor 154). The original builds the
// cutscene synchronously; frame-spanning steps become one timeline handed to the engine with run().
fn janemba_handover(_arg) {
    op_now(0x30, []);
    op_now(0x91, []);
    let s = [face(0, 149), op(0x57, [149, 30, 60]), wait(50)];
    if current_character() == 0 {          // native 0x15
        s += [op(0x4e, [5, 1]), op(0x95, [4]), op(0x5d, [0, 196, 26])];
        set_p(0, 1);
    } else {
        s += [op(0x4e, [1, 5]), op(0x95, [0]), op(0x5d, [0, 196, 26])];
        set_p(0, 5);
    }
    let a = p(0);
    s += [
        op(0x5d, [a, 100, 150]), face(a, 149), op(0x31, [a, 10]), op(0x4f, [149, 1]),
        op(0x4d, [148]), op(0x4d, [147]), op(0x4d, [152]), op(0x7b, [50]),
        op(0x60, [154, 130, 150]),
        // natives 0x38 .. 0x3a bracket a block of simultaneous commands
        parallel([transform(154, 4), walk_to(a, 70, 150)]),
        wait(30),
        op(0x51, [a, 16, 3]), walk_to(0, 90, 150), face(0, 2),
        then_call("after_handover", 0)
    ];
    run(sequence(s));
    if current_character() == 0 { 3 } else { 2 }
}

fn after_handover(_arg) {
    set_flag(300);
    run(sequence([say(154, 1201), play_sfx(12), start_battle(154)]));
    0
}

// Script #2 (equipment effects; abridged after 0x8b).
fn item_use(_arg) {
    if p(0) == 0 {
        if is_equipped(144) {
            op_now(0x21, [48]);
            op_now(0x21, [49]);
            set_p(0, rand(5));
            switch p(0) {
                0 => set_p(1, 7),
                1 => set_p(1, 10),
                2 => set_p(1, 11),
                3 => set_p(1, 59),
                4 => set_p(1, 61),
            }
            op_now(0x1f, [p(1)]);
        }
        if is_equipped(28) {
            set_stat(14, stat(14) + 2);
        } else if is_equipped(15) || is_equipped(152) {
            set_stat(12, stat(12) + 1);
        } else {
            set_stat(14, stat(14) + 1);
        }
        if is_equipped(141) {
            set_stat(16, stat(16) + 1);
        }
    }
    0
}

// Script #497 (enemy drop: run_script(497, loot_level, forced_item)). Note the original `and` is not
// short-circuit: chance() is always rolled, so it is hoisted to keep the RNG stream identical.
fn loot(_arg) {
    if chance(1) { return 0; }
    let r = 1 + rand(100);
    set_g(0, if r <= 5 { 8 } else if r <= 10 { 7 } else if r <= 20 { 6 } else if r <= 30 { 5 }
             else if r <= 40 { 4 } else if r <= 55 { 3 } else if r <= 75 { 2 } else { 1 });
    if is_equipped(121) { set_g(0, g(0) + 1); }
    if is_equipped(136) { set_g(0, g(0) + 2); }
    if is_equipped(151) { set_g(0, g(0) + 3); }
    if is_equipped(138) { set_g(0, g(0) - 1); }
    if is_equipped(143) { set_g(0, g(0) - 2); }
    if is_equipped(131) { set_g(0, g(0) - 3); }
    if g(0) < 1 { set_g(0, 1); } else if g(0) > 8 { set_g(0, 8); }
    let c = chance(5);
    if p(1) != -1 && p(1) != -2 && c {
        if !is_equipped(p(1)) {
            give_item(p(1), 1);
            set_g(0, g(0) - 1);
            if g(0) == 0 { return 0; }
        }
    }
    set_g(1, if p(1) != -2 { 0 } else { 1 });
    set_g(2, 0);
    for _i in 0..g(0) {
        let c = chance(85);
        if c || (g(2) != 0 && g(1) != 0) {
            run_script(500, p(0));
        } else {
            let first = g(1) == 0;
            let c2 = chance(25);
            if first && c2 { set_p(1, 0); run_script(496, p(0)); } else { run_script(499, p(0)); }
        }
    }
    0
}

// Per-frame actor handler (benchmark): 50 calls per frame.
fn on_update(a) {
    let pl = player(1);
    let dx = actor_x(a) - actor_x(pl);
    let dy = actor_y(a) - actor_y(pl);
    let d = abs(dx) + abs(dy);
    if d < 64 {
        set_state(a, 1);
        if chance(10) { run(face(a, 3)); }
    } else {
        set_state(a, 0);
    }
    d
}
