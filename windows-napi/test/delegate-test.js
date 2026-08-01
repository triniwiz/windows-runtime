// End-to-end test of the NapiDelegate COM bridge: a JS function wrapped as a COM delegate,
// fired through the real vtable (as a WinRT event source would), args marshaled per type.
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

// Numeric arg marshaling per declared param type.
let captured = null;
const d1 = ns.makeDelegate((a, b, c) => { captured = [a, b, c]; }, ['i32', 'u32', 'bool']);
check('invoke hr', ns.invokeDelegate(d1, -5, 4000000000, 1), 0);
check('i32 arg', captured[0], -5);
check('u32 arg', captured[1], 4000000000);
check('bool arg', captured[2], true);

// Fewer raw args than registers: extra register values are ignored beyond param_types.len().
let count = null;
const d2 = ns.makeDelegate((...a) => { count = a.length; }, ['u8']);
ns.invokeDelegate(d2, 200, 999, 999);
check('arity respects param_types', count, 1);

// Pointer param: 0 -> null; a real COM pointer that is not IInspectable (another delegate)
// falls back to an external after the typed-proxy probe.
let ptrArg = 'unset';
const d3 = ns.makeDelegate(p => { ptrArg = p; }, ['pointer']);
ns.invokeDelegate(d3, 0, 0, 0);
check('null pointer arg', ptrArg, null);
const comPtr = ns.makeDelegate(() => {}, []);
ns.invokeDelegate(d3, comPtr, 0, 0);
check('non-inspectable COM arg is external', typeof ptrArg, 'object');
ns.releaseDelegate(comPtr);

// A throwing callback must not propagate into the COM caller; error is captured.
const d4 = ns.makeDelegate(() => { throw new Error('delegate boom'); }, []);
check('throwing callback hr', ns.invokeDelegate(d4, 0, 0, 0), 0);
check('error captured', ns.lastError(), e => typeof e === 'string' && e.includes('delegate boom'));

// Refcounting: QI/AddRef aren't exposed here, but create->release drops to zero and frees.
check('release d1', ns.releaseDelegate(d1), 0);
check('release d2', ns.releaseDelegate(d2), 0);
check('release d3', ns.releaseDelegate(d3), 0);
check('release d4', ns.releaseDelegate(d4), 0);

// Delegate still works after unrelated GC pressure (the napi_ref pins the function).
let gcHit = 0;
let d5 = ns.makeDelegate(() => { gcHit++; }, []);
for (let i = 0; i < 50000; i++) { ({ x: i }); }
ns.invokeDelegate(d5, 0, 0, 0);
check('survives GC pressure', gcHit, 1);
ns.releaseDelegate(d5);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
