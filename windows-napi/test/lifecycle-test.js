// Memory / GC / lifecycle — parity with napi-android (testGC, testWeakRef,
// testReleaseNativeCounterpart, testsMemoryManagement) and napi-ios (test/cli/memory/*).
// Exercises the class of bug that bit us on QuickJS (the finalizer double-free): heavy
// create/discard churn + forced GC must not corrupt state or crash, and instances/results
// must stay correct across collections.
//
// Run with: node --expose-gc lifecycle-test.js
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

const Windows = ns.getNamespace('Windows');
const { JsonObject, JsonValue } = Windows.Data.Json;
const gc = typeof global.gc === 'function' ? global.gc : () => {};
const HAS_GC = typeof global.gc === 'function';
check('run with --expose-gc (GC forced, not just churned)', HAS_GC, true);

// 1. Instance churn: thousands of create+use+discard, GC between rounds. No crash, values correct.
let churnOk = true;
for (let round = 0; round < 40 && churnOk; round++) {
  for (let i = 0; i < 100; i++) {
    if (JsonValue.CreateNumberValue(i).GetNumber() !== i) { churnOk = false; break; }
  }
  gc();
}
check('4000 instance create/use/discard + GC rounds stay correct', churnOk, true);

// 2. Repeated static-method resolution + call (the exact double-free repro shape), then GC.
for (let i = 0; i < 300; i++) { void JsonValue.CreateNumberValue; JsonValue.CreateNumberValue(i).GetNumber(); }
gc(); gc();
check('repeated static-method resolution stable after GC', JsonValue.CreateNumberValue(7).GetNumber(), 7);

// 3. Long-lived instance held ACROSS multiple GCs stays valid (Arc/COM not collected/corrupted).
const held = JsonObject.Parse('{"k":42,"s":"hi"}');
for (let i = 0; i < 5; i++) gc();
check('held instance valid after 5 GCs (number)', held.GetNamedNumber('k'), 42);
check('held instance valid after 5 GCs (string)', held.GetNamedString('s'), 'hi');

// 4. Object graph churn: build + stringify + drop many composite objects, GC between.
let graphOk = true;
for (let round = 0; round < 20 && graphOk; round++) {
  const o = new JsonObject();
  for (let i = 0; i < 20; i++) o.SetNamedValue('k' + i, JsonValue.CreateNumberValue(i));
  if (o.GetNamedNumber('k19') !== 19) graphOk = false;
  if (!o.Stringify().includes('"k0"')) graphOk = false;
  gc();
}
check('composite object churn + GC stays correct', graphOk, true);

// 5. WeakRef: a dropped instance is reclaimable (timing not guaranteed — assert no crash + type).
if (typeof WeakRef === 'function') {
  let wr;
  (function () { wr = new WeakRef(JsonValue.CreateNumberValue(123)); })();
  for (let i = 0; i < 5; i++) gc();
  const d = wr.deref();
  check('WeakRef deref after GC is undefined or a live proxy', d,
        v => v === undefined || v.GetNumber() === 123);
} else {
  check('WeakRef available', typeof WeakRef, 'function');
}

// 6. Post-churn sanity: the whole surface still works after all the GC pressure.
check('runtime healthy after churn', JsonObject.Parse('{"z":9}').GetNamedNumber('z'), 9);

console.log(`${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
