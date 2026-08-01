// E2E: WinRT collections (IVector via JsonArray) with native ergonomics — .Size, .length,
// indexed access v[i], for-of, spread, Array.from — all through the interface-instance proxy.
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

const { JsonArray } = ns.getNamespace('Windows').Data.Json;
const arr = JsonArray.Parse('[10, 20, 30]');

// Interface property + method work directly (base capability).
check('Size property', arr.Size, 3);
check('GetAt(1).GetNumber()', arr.GetAt(1).GetNumber(), 20);

// Ergonomic sugar layered on top.
check('length sugar', arr.length, 3);
check('indexed [0]', arr[0].GetNumber(), 10);
check('indexed [2]', arr[2].GetNumber(), 30);

// for-of iteration via Symbol.iterator (materialized through Size + GetAt).
const viaForOf = [];
for (const el of arr) viaForOf.push(el.GetNumber());
check('for-of', JSON.stringify(viaForOf), '[10,20,30]');

// Spread + Array.from.
check('spread', [...arr].map((e) => e.GetNumber()).join(','), '10,20,30');
check('Array.from', Array.from(arr, (e) => e.GetNumber()).join(','), '10,20,30');

// map/reduce over spread proves the elements are usable proxies.
check('sum via spread', [...arr].reduce((a, e) => a + e.GetNumber(), 0), 60);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
