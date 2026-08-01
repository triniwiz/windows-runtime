// E2E: real WinRT calls from Node through the napi-ported marshaling pipeline
// (metadata lookup → activation factory → MethodCall::call_napi → return conversion).
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}
function throws(name, fn) {
  try { fn(); console.log(`FAIL ${name}: did not throw`); fail++; }
  catch { pass++; }
}

const JV = 'Windows.Data.Json.JsonValue';
const JO = 'Windows.Data.Json.JsonObject';

// Static factory method with a string arg → IJsonValue instance (external).
const sv = ns.callStaticMethod(JV, 'CreateStringValue', ['hello']);
check('CreateStringValue returns typed proxy', sv.__typeName__, 'Windows.Data.Json.JsonValue');

// Instance method with a string return → HSTRING marshaled back.
check('Stringify round-trips', ns.callInstanceMethod(sv, JV, 'Stringify', []), '"hello"');

// Number arg + number return.
const nv = ns.callStaticMethod(JV, 'CreateNumberValue', [3.5]);
check('GetNumber', ns.callInstanceMethod(nv, JV, 'GetNumber', []), 3.5);

// Boolean arg + boolean return.
const bv = ns.callStaticMethod(JV, 'CreateBooleanValue', [true]);
check('GetBoolean', ns.callInstanceMethod(bv, JV, 'GetBoolean', []), true);

// Unicode string round-trip through HSTRING.
const uv = ns.callStaticMethod(JV, 'CreateStringValue', ['héllo✓🚀']);
check('unicode GetString', ns.callInstanceMethod(uv, JV, 'GetString', []), 'héllo✓🚀');

// JsonObject.Parse (static) → instance → GetNamedNumber(String) with an argument.
const obj = ns.callStaticMethod(JO, 'Parse', ['{"a": 41.5, "s": "x"}']);
check('Parse returns typed proxy', obj.__typeName__, 'Windows.Data.Json.JsonObject');
check('GetNamedNumber(a)', ns.callInstanceMethod(obj, JO, 'GetNamedNumber', ['a']), 41.5);
check('GetNamedString(s)', ns.callInstanceMethod(obj, JO, 'GetNamedString', ['s']), 'x');
check('Stringify object', ns.callInstanceMethod(obj, JO, 'Stringify', []), s => s.includes('"a"') && s.includes('41.5'));

// Failing WinRT call surfaces as a JS error (bad JSON → HRESULT failure).
throws('Parse invalid json throws', () => ns.callStaticMethod(JO, 'Parse', ['not json']));

// Unknown type / method are clean JS errors.
throws('unknown class throws', () => ns.callStaticMethod('Windows.No.Such.Class', 'Foo', []));
throws('unknown method throws', () => ns.callStaticMethod(JV, 'NoSuchMethod', []));

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
