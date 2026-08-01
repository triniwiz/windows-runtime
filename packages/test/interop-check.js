// Standalone-engine check for the shared NSWinRT.interop surface (installed by
// install_globals; same source as the Node package's nswinrt.js `interop`):
//   nativescript-windows packages/interop-check.js
// Prints "interop-check: N passed, M failed"; throws (nonzero exit) on any failure.
'use strict';

var interop = globalThis.NSWinRT && globalThis.NSWinRT.interop;
var pass = 0, fail = 0;
function check(name, ok) {
  if (ok) { pass += 1; }
  else { fail += 1; console.error('FAIL ' + name); }
}

check('interop installed', !!interop);
check('global alias', globalThis.interop === interop);

// Typed concrete values round-trip through a WinRT Object-typed parameter.
var ps = new Windows.Foundation.Collections.PropertySet();
ps.Insert('d', interop.double(2.5));
check('double round-trip', ps.Lookup('d') === 2.5);
ps.Insert('i', interop.int(42));
check('int round-trip', ps.Lookup('i') === 42);
ps.Insert('b', interop.bool(true));
check('bool round-trip', ps.Lookup('b') === true);
var ref = interop.reference('Double', 1.25);
check('reference shape', ref !== null && typeof ref === 'object');
ps.Insert('r', ref);
check('reference round-trip', ps.Lookup('r') === 1.25);
check('unknown type null', globalThis.__nsTypedValue('NoSuchType', 1) === null);

// Pointer / OutParam helpers.
var p = new interop.Pointer(null);
check('Pointer.isNull', p.isNull() === true);
check('isPointer', interop.isPointer(p) === true);
var o = interop.out('Int32', 5);
check('out/isOut', interop.isOut(o) && o.type === 'Int32' && o.value === 5);

// Buffer helpers + the buffer→pointer natives.
var u8 = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]);
check('byteLengthOf', interop.byteLengthOf(u8) === 8);
interop.writeI32(u8, 4, -2);
check('readI32', interop.readI32(u8, 4) === -2);
check('readU8', interop.readU8(u8, 2) === 3);
var bp = interop.pointerFromBuffer(u8);
check('pointerFromBuffer', bp !== null);
var key = interop.pointerKey(bp);
check('pointerKey', typeof key === 'string' && key.indexOf('0x') === 0 && key !== '0x0');
check('pointerKey(null)', interop.pointerKey(null) === '0x0');
check('track/resolve', interop.resolveTrackedBuffer(interop.trackBufferSource(u8)) === u8);

// WinRT DateTime ticks (Number fallback tolerated on engines without BigInt).
var ticks = interop.toWinRTDateTimeTicks(0);
check('epoch ticks', typeof BigInt === 'function'
  ? ticks === BigInt('116444736000000000')
  : ticks === 116444736000000000);
check('ticks round-trip',
  interop.fromWinRTDateTimeTicks(interop.toWinRTDateTimeTicks(1752768000000)).getTime() === 1752768000000);

// uuid + winmd helpers over the natives.
var u = interop.uuid();
check('uuid', typeof u === 'string' && u.length === 36);
check('scanWinmdDir missing', interop.scanWinmdDir('C:\\no\\such\\dir') === 0);

console.log('interop-check: ' + pass + ' passed, ' + fail + ' failed');
if (fail) { throw new Error('interop-check failed'); }
