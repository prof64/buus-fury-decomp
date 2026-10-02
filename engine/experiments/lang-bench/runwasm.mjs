// node runwasm.mjs — run the wasm builds, print digests (compare with `bench` native output) and timings.
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";

const dir = new URL("./target/wasm32-unknown-unknown/release-wasm/", import.meta.url);
for (const name of ["wasm_mini", "wasm_rhai", "wasm_boa"]) {
  const bytes = readFileSync(new URL(`${name}.wasm`, dir));
  const mod = await WebAssembly.compile(bytes);
  const imports = {};
  const names = [];
  for (const imp of WebAssembly.Module.imports(mod)) {
    names.push(`${imp.module}.${imp.name}`);
    imports[imp.module] ??= {};
    imports[imp.module][imp.name] = () => { throw new Error(`import ${imp.module}.${imp.name} called`); };
  }
  const inst = await WebAssembly.instantiate(mod, imports);
  const ex = inst.exports;
  const hex = (v) => BigInt.asUintN(64, v).toString(16).padStart(16, "0");
  let line = `${name}: ${bytes.length} bytes, imports [${names.join(", ")}]`;
  for (const f of ["updates_digest", "scenario_digest"]) {
    if (!ex[f]) continue;
    ex[f](); // warm up
    const t = performance.now();
    const reps = 5;
    let d;
    for (let i = 0; i < reps; i++) d = ex[f]();
    const ms = (performance.now() - t) / reps;
    line += `\n  ${f} = ${hex(d)}  (${ms.toFixed(2)} ms/run)`;
  }
  console.log(line);
}
