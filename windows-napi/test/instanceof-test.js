// `instanceof` operator — parity with napi-android testsInstanceOfOperator.
// A WinRT class proxy should behave like a constructor for `instanceof`: false for primitives,
// null/undefined, plain JS objects, and unrelated WinRT types; true for its own instances.
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

const Windows = ns.getNamespace('Windows');
const { JsonObject, JsonValue } = Windows.Data.Json;

// Negatives (these must be false and must not throw).
check('number instanceof WinRT class', 5 instanceof JsonObject, false);
check('null instanceof WinRT class', null instanceof JsonObject, false);
check('undefined instanceof WinRT class', undefined instanceof JsonObject, false);
check('plain object instanceof WinRT class', ({}) instanceof JsonObject, false);
function Poly() {}
check('plain JS class instance instanceof WinRT class', new Poly() instanceof JsonObject, false);

// Positives / cross-type.
const obj = new JsonObject();
check('instance instanceof its own class', obj instanceof JsonObject, true);
const parsed = JsonObject.Parse('{"x":1}');
check('typed-return instance instanceof its class', parsed instanceof JsonObject, true);
const num = JsonValue.CreateNumberValue(1);
check('unrelated WinRT type not instanceof class', num instanceof JsonObject, false);
check('JsonValue instanceof JsonValue', num instanceof JsonValue, true);

console.log(`${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
