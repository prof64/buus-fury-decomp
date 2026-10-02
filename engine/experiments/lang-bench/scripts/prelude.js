// Plain-data action constructors (no engine calls). The engine walks these objects in run().
"use strict";
const say = (speaker, text) => ({ k: "say", a: [speaker, text] });
const wait = (frames) => ({ k: "wait", a: [frames] });
const walk_to = (actor, x, y) => ({ k: "walk_to", a: [actor, x, y] });
const face = (actor, dir) => ({ k: "face", a: [actor, dir] });
const transform = (actor, form) => ({ k: "transform", a: [actor, form] });
const play_sfx = (id) => ({ k: "play_sfx", a: [id] });
const start_battle = (id) => ({ k: "start_battle", a: [id] });
const flag_later = (id) => ({ k: "set_flag", a: [id] });
const op = (id, args) => ({ k: "op", id, a: args });
const sequence = (list) => ({ k: "sequence", list });
const parallel = (list) => ({ k: "parallel", list });
const then_call = (name, arg) => ({ k: "call", name, a: [arg] });
