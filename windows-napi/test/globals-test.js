// Tests install_globals: __time always installed; host performance/console preserved on Node;
// console timer methods work through the shared runtime state when invoked directly.
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${String(got)}, expected ${String(expected)}`); fail++; }
}

const hostPerformanceNow = performance.now;
const hostConsoleLog = console.log;

ns.installGlobals();

// __time installed and monotonic.
check('__time exists', typeof globalThis.__time, 'function');
const t1 = globalThis.__time();
check('__time returns ms', t1, v => typeof v === 'number' && v > 0);
for (let i = 0; i < 100000; i++) { Math.sqrt(i); }
check('__time monotonic', globalThis.__time() >= t1, true);

// Host-provided globals must NOT be overwritten (install-if-missing contract).
check('host performance.now preserved', performance.now === hostPerformanceNow, true);
check('host console preserved', console.log === hostConsoleLog, true);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
