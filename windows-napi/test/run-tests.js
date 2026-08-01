// Runs every *-test.js suite in its own process (they each call process.exit) and aggregates the
// "N passed, N failed" tallies. lifecycle-test needs --expose-gc to force GC.
//   node test/run-tests.js
const { execFileSync } = require('child_process');
const fs = require('fs');

// All structured suites (value-test, value-test2, …) plus the older smoke tests (proxy/smoke).
const suites = fs.readdirSync(__dirname).filter((f) => /-test\d*\.js$/.test(f)).sort();
let pass = 0, fail = 0;
const broken = [];

for (const s of suites) {
  const args = s === 'lifecycle-test.js' ? ['--expose-gc', s] : [s];
  let out = '', crashed = false;
  try {
    out = execFileSync('node', args, { cwd: __dirname, encoding: 'utf8' });
  } catch (e) {
    out = (e.stdout || '') + '\n' + (e.stderr || '');
    crashed = true;
  }
  const m = out.match(/(\d+) passed, (\d+) failed/);
  if (m) {
    pass += +m[1]; fail += +m[2];
    console.log(`${s.padEnd(22)} ${m[1]} passed, ${m[2]} failed`);
  } else if (!crashed) {
    console.log(`${s.padEnd(22)} ok (smoke, no tally)`); // proxy/smoke: pass if exit 0
  } else {
    broken.push(s);
    console.log(`${s.padEnd(22)} CRASHED`);
  }
}

console.log(`\nTOTAL: ${pass} passed, ${fail} failed across ${suites.length} suites`);
if (broken.length) console.log(`crashed: ${broken.join(', ')}`);
process.exit(fail || broken.length ? 1 : 0);
