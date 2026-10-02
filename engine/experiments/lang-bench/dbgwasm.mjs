// node dbgwasm.mjs — run the symbolized Boa build to see where it traps.
import { readFileSync } from "node:fs";
const bytes = readFileSync(new URL("./target/wasm32-unknown-unknown/wasm-dbg/wasm_boa.wasm", import.meta.url));
const mod = await WebAssembly.compile(bytes);
const inst = await WebAssembly.instantiate(mod, {});
try {
  console.log(inst.exports.updates_digest());
} catch (e) {
  console.log(e.stack.split("\n").slice(0, 25).join("\n"));
}
