// E2E: native interop.* utilities — UUID, winmd registration, zero-copy IBuffer→ArrayBuffer.
const { Windows, interop } = require('../nswinrt.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

// UUID: well-formed and unique.
const u1 = interop.uuid();
const u2 = interop.uuid();
check('uuid format', u1, (s) => /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(s));
check('uuid unique', u1 !== u2, true);

// winmd: scanning a non-existent dir returns 0 (no throw); scanning cwd is safe.
check('scanWinmdDir missing', interop.scanWinmdDir('C:\\no\\such\\dir'), 0);
check('scanWinmdDir cwd is number', typeof interop.scanWinmdDir(process.cwd()), 'number');

// arrayBufferFromBuffer: create a real IBuffer via CryptographicBuffer, alias it, read bytes.
const CryptographicBuffer = Windows.Security.Cryptography.CryptographicBuffer;
const winBuf = CryptographicBuffer.DecodeFromHexString('01ff10203040');
check('IBuffer created', winBuf.__typeName__, s => typeof s === 'string');
check('IBuffer.Length', winBuf.Length, 6);

const ab = interop.arrayBufferFromBuffer(winBuf);
check('returns ArrayBuffer', ab instanceof ArrayBuffer, true);
check('ArrayBuffer byteLength', ab.byteLength, 6);
const view = new Uint8Array(ab);
check('bytes[0]', view[0], 0x01);
check('bytes[1]', view[1], 0xff);
check('bytes[2]', view[2], 0x10);
check('bytes[5]', view[5], 0x40);

// Empty buffer → empty ArrayBuffer (no crash).
const empty = CryptographicBuffer.CreateFromByteArray(new Uint8Array(0));
check('empty IBuffer → empty AB', interop.arrayBufferFromBuffer(empty).byteLength, 0);

// The surface is the shared NSWinRT.interop (one source with the standalone engines).
check('NSWinRT.interop is the surface', globalThis.NSWinRT.interop === interop, true);
check('globalThis.interop alias', globalThis.interop === interop, true);

// Typed concrete values (__nsTypedValue): shape is { handle: external }, and a WinRT
// Object-typed round-trip through PropertySet unboxes to the original primitive.
const dv = interop.double(2.5);
check('double() shape', dv !== null && typeof dv === 'object' && 'handle' in dv, true);
const ps = new Windows.Foundation.Collections.PropertySet();
ps.Insert('d', dv);
check('double round-trip', ps.Lookup('d'), 2.5);
ps.Insert('i', interop.int(42));
check('int round-trip', ps.Lookup('i'), 42);
ps.Insert('b', interop.bool(true));
check('bool round-trip', ps.Lookup('b'), true);
ps.Insert('f', interop.float(1.5));
check('float round-trip', ps.Lookup('f'), 1.5);
ps.Insert('l', interop.long(1234));
check('long round-trip', ps.Lookup('l'), 1234);
check('guid() shape', interop.guid(u1), (v) => v !== null && 'handle' in v);
check('dateTime() shape', interop.dateTime(new Date()), (v) => v !== null && 'handle' in v);
check('timeSpan() shape', interop.timeSpan(1500), (v) => v !== null && 'handle' in v);
check('unknown type → null', globalThis.__nsTypedValue('NoSuchType', 1), null);

// reference() — explicit IReference<T> boxing (same native as __nsCreateReference).
const ref = interop.reference('Double', 1.25);
check('reference() shape', ref !== null && 'handle' in ref, true);
ps.Insert('r', ref);
check('reference round-trip', ps.Lookup('r'), 1.25);
check('reference fully-qualified name', interop.reference('Windows.Foundation.DateTime', Date.now()), (v) => v !== null && 'handle' in v);

// Pointer / OutParam JS helpers.
const p0 = new interop.Pointer(null);
check('Pointer null', p0.isNull(), true);
check('Pointer toString', String(p0), '[Pointer null]');
check('isPointer', interop.isPointer(p0), true);
check('pointer() wraps', interop.isPointer(interop.pointer(dv.handle)), true);
check('handleOf(Pointer)', interop.handleOf(interop.pointer(dv.handle)), dv.handle);
check('handleOf passthrough', interop.handleOf(7), 7);
const op = interop.out('Int32', 5);
check('out() isOut', interop.isOut(op), true);
check('out() type', op.type, 'Int32');
check('out() value', op.value, 5);
check('out(value only)', interop.out(9).value, 9);
check('isOut(plain)', interop.isOut({}), false);

// Buffer view helpers (pure JS over DataView).
const u8 = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]);
check('asBufferSource view', interop.asBufferSource(u8), u8);
check('asUint8View of AB', interop.asUint8View(u8.buffer).length, 8);
check('asDataView', interop.asDataView(u8).byteLength, 8);
check('byteLengthOf', interop.byteLengthOf(u8), 8);
check('byteOffsetOf', interop.byteOffsetOf(new Uint8Array(u8.buffer, 4)), 4);
check('readU8', interop.readU8(u8, 2), 3);
interop.writeU8(u8, 0, 0xff);
check('writeU8', u8[0], 0xff);
interop.writeI32(u8, 4, -2);
check('readI32', interop.readI32(u8, 4), -2);
const f64buf = new Uint8Array(8);
interop.writeF64(f64buf, 0, 3.25);
check('readF64', interop.readF64(f64buf, 0), 3.25);

// WinRT DateTime ticks (100ns since 1601): epoch constant and Date round-trip.
check('epoch ticks', interop.toWinRTDateTimeTicks(new Date(0)), 116444736000000000n);
const nowMs = 1752768000000;
check('ticks round-trip', interop.fromWinRTDateTimeTicks(interop.toWinRTDateTimeTicks(nowMs)).getTime(), nowMs);

// Pointer keys + buffer→pointer natives.
check('pointerKey(null)', interop.pointerKey(null), '0x0');
check('pointerKey(proxy)', interop.pointerKey(winBuf), (s) => /^0x[0-9a-f]+$/.test(s));
check('pointerKey(typed value)', interop.pointerKey(dv), (s) => /^0x[0-9a-f]+$/.test(s) && s !== '0x0');
const bufPtr = interop.pointerFromBuffer(u8);
check('pointerFromBuffer', bufPtr !== null, true);
check('pointerKey(bufPtr)', interop.pointerKey(bufPtr), (s) => /^0x[0-9a-f]+$/.test(s) && s !== '0x0');
// View into the same store at an offset gets a distinct address.
check('offset view distinct ptr', interop.pointerKey(interop.pointerFromBuffer(new Uint8Array(u8.buffer, 4))) !== interop.pointerKey(bufPtr), true);
check('trackBufferSource → resolve', interop.resolveTrackedBuffer(interop.trackBufferSource(u8)), u8);

console.log(`\n${pass} passed, ${fail} failed`);
process.exitCode = fail ? 1 : 0;
