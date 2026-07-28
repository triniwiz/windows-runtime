// Parity tests for the ported console formatter (format_item + table rendering).
// Expected strings mirror the rusty_v8 handle_item_log semantics.
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : got === expected;
  if (ok) pass++;
  else { console.log(`FAIL ${name}:\n  got      ${JSON.stringify(got)}\n  expected ${JSON.stringify(expected)}`); fail++; }
}

// Primitives stringify via ToString.
check('number', ns.formatValue(42, false), '42');
check('string', ns.formatValue('hi', false), 'hi');
check('bool', ns.formatValue(true, false), 'true');
check('null', ns.formatValue(null, false), 'null');
check('undefined', ns.formatValue(undefined, false), 'undefined');

// Arrays: bracketed, comma-separated, nested.
check('array', ns.formatValue([1, 2, 3], false), '[1, 2, 3]');
check('nested array', ns.formatValue([1, [2, 3]], false), '[1, [2, 3]]');
check('empty array', ns.formatValue([], false), '[]');

// Functions stringify to source.
check('function', ns.formatValue(function foo() {}, false), s => s.includes('function foo'));

// Plain objects: shallow { k: v } summary.
check('object shallow', ns.formatValue({ a: 1, b: 'x' }, false), '{ a: 1, b: x }');
check('object nested fn', ns.formatValue({ f: () => {} }, false), '{ f: () }');
check('object nested obj', ns.formatValue({ o: { x: 1 } }, false), '{ o: {"x":1} }');

// TypedArray views stringify via ToString (v8 parity: "1,2,3").
check('typed array', ns.formatValue(new Uint8Array([1, 2, 3]), false), '1,2,3');

// rich (console.dir) mode: multi-line listing.
const dirOut = ns.formatValue({ a: 1, f: () => {} }, true);
check('dir lists props', dirOut, s => s.includes('  a: 1\n') && s.includes('  f: ()\n'));

// Getter that throws is caught, not propagated.
const throwing = {};
Object.defineProperty(throwing, 'bad', { get() { throw new Error('nope'); }, enumerable: true });
check('getter threw', ns.formatValue(throwing, false), s => s.includes('bad: <getter threw>'));

// Circular structures degrade to #CR (via JSON.stringify failure contract).
const circ = { a: 1 }; circ.self = circ;
check('circular object', ns.formatValue(circ, false), s => s.includes('#CR') || s.includes('self:'));

// console.table rendering (shared render_table — box-drawing output).
const t1 = ns.tableFor([{ x: 1, y: 2 }, { x: 3, y: 4 }], null);
check('table headers', t1, s => s.includes('(index)') && s.includes(' x ') && s.includes(' y '));
check('table rows', t1, s => s.includes(' 1 ') && s.includes(' 4 '));
check('table borders', t1, s => s.includes('┌') && s.includes('┘'));
const t2 = ns.tableFor([{ x: 1, y: 2 }], ['x']);
check('table column filter', t2, s => s.includes(' x ') && !s.includes(' y '));
const t3 = ns.tableFor({ k1: 'v1', k2: 'v2' }, null);
check('table from object', t3, s => s.includes('k1') && s.includes('v1') && s.includes('Values'));
check('table primitives array', ns.tableFor([7, 8], null), s => s.includes('Values') && s.includes(' 7 '));
check('table empty array', ns.tableFor([], null), '(empty)\n');

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
