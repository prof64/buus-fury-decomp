// Same translations as story.rhai, in JavaScript (types: api.d.ts). Actions are plain objects built by
// the prelude (prelude.js); run() hands them to the engine. All values are integers (JS numbers < 2^53).
"use strict";

// Script #450 (Janemba -> Super Janemba handover).
function janemba_handover(_arg) {
  op_now(0x30, []);
  op_now(0x91, []);
  const s = [face(0, 149), op(0x57, [149, 30, 60]), wait(50)];
  if (current_character() === 0) {
    s.push(op(0x4e, [5, 1]), op(0x95, [4]), op(0x5d, [0, 196, 26]));
    set_p(0, 1);
  } else {
    s.push(op(0x4e, [1, 5]), op(0x95, [0]), op(0x5d, [0, 196, 26]));
    set_p(0, 5);
  }
  const a = p(0);
  s.push(
    op(0x5d, [a, 100, 150]), face(a, 149), op(0x31, [a, 10]), op(0x4f, [149, 1]),
    op(0x4d, [148]), op(0x4d, [147]), op(0x4d, [152]), op(0x7b, [50]),
    op(0x60, [154, 130, 150]),
    parallel([transform(154, 4), walk_to(a, 70, 150)]),
    wait(30),
    op(0x51, [a, 16, 3]), walk_to(0, 90, 150), face(0, 2),
    then_call("after_handover", 0),
  );
  run(sequence(s));
  return current_character() === 0 ? 3 : 2;
}

function after_handover(_arg) {
  set_flag(300);
  run(sequence([say(154, 1201), play_sfx(12), start_battle(154)]));
  return 0;
}

// Script #2 (abridged).
function item_use(_arg) {
  if (p(0) === 0) {
    if (is_equipped(144)) {
      op_now(0x21, [48]);
      op_now(0x21, [49]);
      set_p(0, rand(5));
      switch (p(0)) {
        case 0: set_p(1, 7); break;
        case 1: set_p(1, 10); break;
        case 2: set_p(1, 11); break;
        case 3: set_p(1, 59); break;
        case 4: set_p(1, 61); break;
      }
      op_now(0x1f, [p(1)]);
    }
    if (is_equipped(28)) set_stat(14, stat(14) + 2);
    else if (is_equipped(15) || is_equipped(152)) set_stat(12, stat(12) + 1);
    else set_stat(14, stat(14) + 1);
    if (is_equipped(141)) set_stat(16, stat(16) + 1);
  }
  return 0;
}

// Script #497 (loot).
function loot(_arg) {
  if (chance(1)) return 0;
  const r = 1 + rand(100);
  set_g(0, r <= 5 ? 8 : r <= 10 ? 7 : r <= 20 ? 6 : r <= 30 ? 5 : r <= 40 ? 4 : r <= 55 ? 3 : r <= 75 ? 2 : 1);
  if (is_equipped(121)) set_g(0, g(0) + 1);
  if (is_equipped(136)) set_g(0, g(0) + 2);
  if (is_equipped(151)) set_g(0, g(0) + 3);
  if (is_equipped(138)) set_g(0, g(0) - 1);
  if (is_equipped(143)) set_g(0, g(0) - 2);
  if (is_equipped(131)) set_g(0, g(0) - 3);
  if (g(0) < 1) set_g(0, 1); else if (g(0) > 8) set_g(0, 8);
  const c = chance(5);
  if (p(1) !== -1 && p(1) !== -2 && c) {
    if (!is_equipped(p(1))) {
      give_item(p(1), 1);
      set_g(0, g(0) - 1);
      if (g(0) === 0) return 0;
    }
  }
  set_g(1, p(1) !== -2 ? 0 : 1);
  set_g(2, 0);
  const n = g(0);
  for (let i = 0; i < n; i++) {
    const c1 = chance(85);
    if (c1 || (g(2) !== 0 && g(1) !== 0)) {
      run_script(500, p(0));
    } else {
      const first = g(1) === 0;
      const c2 = chance(25);
      if (first && c2) { set_p(1, 0); run_script(496, p(0)); } else run_script(499, p(0));
    }
  }
  return 0;
}

function on_update(a) {
  const pl = player(1);
  const dx = actor_x(a) - actor_x(pl);
  const dy = actor_y(a) - actor_y(pl);
  const d = Math.abs(dx) + Math.abs(dy);
  if (d < 64) {
    set_state(a, 1);
    if (chance(10)) run(face(a, 3));
  } else {
    set_state(a, 0);
  }
  return d;
}
