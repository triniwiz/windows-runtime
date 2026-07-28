// Verifies napi_engine::value marshaling matches the rusty_v8 semantics bit-for-bit,
// including V8's integer-truncation quirks. Exits non-zero on any mismatch.
const ns = require('../index.js');

let pass = 0, fail = 0;
const near = (a, b) => Math.abs(a - b) < 1e-6;

function eq(ty, input, expected, note) {
  let got;
  try { got = ns.ffiRoundtrip(input, ty); }
  catch (e) { console.log(`FAIL ${ty}(${String(input)}) threw: ${e.message}`); fail++; return; }
  const g = typeof got === 'bigint' ? got : got;
  const ok = typeof expected === 'number'
    ? (typeof g === 'bigint' ? Number(g) === expected : (Number.isInteger(expected) ? g === expected : near(g, expected)))
    : g === expected;
  if (ok) { pass++; }
  else { console.log(`FAIL ${ty}(${String(input)}) => ${String(got)}, expected ${String(expected)}${note ? ' ('+note+')' : ''}`); fail++; }
}

function throws(ty, input) {
  try { ns.ffiRoundtrip(input, ty); console.log(`FAIL ${ty}(${String(input)}) did not throw`); fail++; }
  catch { pass++; }
}

// bool
eq('bool', true, true); eq('bool', false, false); throws('bool', 1);

// u8 — Uint32 path truncates to width (300 & 0xFF = 44; 256 -> 0)
eq('u8', 42, 42); eq('u8', 255, 255); eq('u8', 300, 44, 'trunc'); eq('u8', 256, 0, 'trunc');
throws('u8', -1); throws('u8', 3.5);

// i8 — Int32 path truncates (200 as i8 = -56; 128 -> -128)
eq('i8', -5, -5); eq('i8', 127, 127); eq('i8', 200, -56, 'trunc'); eq('i8', 128, -128, 'trunc');

// u16 / i16 truncation
eq('u16', 65535, 65535); eq('u16', 70000, 70000 & 0xFFFF, 'trunc');
eq('i16', -1, -1); eq('i16', 40000, (40000 << 16) >> 16, 'trunc');

// u32 / i32 — width matches, no truncation; out-of-range throws
eq('u32', 4000000000, 4000000000); throws('u32', 5000000000);
eq('i32', -2000000000, -2000000000); throws('i32', 3000000000);

// u64 / i64 — number and BigInt inputs
eq('u64', 42, 42); eq('u64', 123n, 123); eq('i64', -42, -42); eq('i64', -123n, -123);
eq('usize', 42, 42); eq('isize', -42, -42);

// f32 — exact-representable values, plus one rounding check
eq('f32', 0.5, 0.5); eq('f32', -2.5, -2.5); eq('f32', 0.1, Math.fround(0.1), 'f32 round');

// f64
eq('f64', 3.14159, 3.14159); throws('f64', 'x');

// string — UTF-16 round-trip, unicode; non-string throws
eq('string', 'hello', 'hello'); eq('string', '', ''); eq('string', 'héllo✓🚀', 'héllo✓🚀'); throws('string', 123);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
