// Typings sketch for the JS/TS option (modder autocomplete). Integers only: `int` is a documentation alias.
type int = number;
type ActorId = int;

interface Action { readonly k: string }

declare function say(speaker: ActorId, text: int): Action;
declare function wait(frames: int): Action;
declare function walk_to(actor: ActorId, x: int, y: int): Action;
declare function face(actor: ActorId, dir: int): Action;
declare function transform(actor: ActorId, form: int): Action;
declare function play_sfx(id: int): Action;
declare function start_battle(enemy: int): Action;
declare function flag_later(flag: int): Action;
declare function op(native: int, args: int[]): Action;
declare function sequence(steps: Action[]): Action;
declare function parallel(steps: Action[]): Action;
declare function then_call(handler: string, arg: int): Action;
declare function run(a: Action): void;

declare function get_flag(flag: int): boolean;
declare function set_flag(flag: int): void;
declare function rand(n: int): int;
declare function chance(percent: int): boolean;
declare function player(n: int): ActorId;
declare function actor_x(a: ActorId): int;
declare function actor_y(a: ActorId): int;
declare function set_state(a: ActorId, s: int): void;
declare function give_item(item: int, n: int): void;
declare function is_equipped(item: int): boolean;
declare function stat(i: int): int;
declare function set_stat(i: int, v: int): void;
declare function current_character(): int;
declare function g(i: int): int;
declare function set_g(i: int, v: int): void;
declare function p(i: int): int;
declare function set_p(i: int, v: int): void;
declare function run_script(idx: int, p0: int): void;
declare function op_now(native: int, args: int[]): void;
