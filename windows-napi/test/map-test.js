// E2E: IMap keyed sugar — m[key] → Lookup, m[key] = v → Insert, `key in m` → HasKey — on
// class-path keyed maps (StringMap, PropertySet: classes whose default interface is a map stay
// on the Proxy path), interface-path maps (IMapView from GetView), and boxed-primitive
// unboxing for PropertySet/ValueSet reads.
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

const { StringMap, PropertySet, ValueSet } = ns.getNamespace('Windows').Foundation.Collections;

// StringMap (IMap<String, String> is its default interface → Proxy path).
const m = new StringMap();
m.Insert('a', '1'); // native method still works
check('native Insert + Size', m.Size, 1);
check('sugar read', m['a'], '1');
m['b'] = '2'; // sugar write → Insert
check('sugar write lands in map', m.HasKey('b'), true);
check('sugar write reads back', m['b'], '2');
m['b'] = '2b'; // overwrite
check('sugar overwrite', m['b'], '2b');
check('in operator (present)', 'b' in m, true);
check('in operator (missing)', 'zz' in m, false);
check('missing key reads undefined', m['zz'], undefined);
check('length sugar', m.length, 2);
check('Size member wins over keys', typeof m.Size, 'number');

// WinRT members take precedence over map keys.
m['Size'] = '99';
check('member precedence on read', m.Size, 3); // 'Size' went into the map, member still wins
check('shadowed key via Lookup', m.Lookup('Size'), '99');

// Interface-path map: GetView returns IMapView`2<String, String> (generic interface instance).
const view = m.GetView();
check('view member (Size)', view.Size, 3);
check('view member (HasKey)', view.HasKey('a'), true);
check('view sugar read', view['a'], '1');
check('view in operator', 'a' in view, true);
// IMapView has no Insert: writes become plain expandos, never touch the map.
view['c'] = 'x';
check('view write is expando (map untouched)', m.HasKey('c'), false);
check('view expando reads back', view['c'], 'x');

// PropertySet / ValueSet (IPropertySet default interface): JS primitives box on Insert and
// unbox (IPropertyValue) on Lookup.
const ps = new PropertySet();
ps['s'] = 'hello';
ps['n'] = 42;
ps['b'] = true;
check('PropertySet string round-trip', ps['s'], 'hello');
check('PropertySet number round-trip', ps['n'], 42);
check('PropertySet bool round-trip', ps['b'], true);
check('PropertySet HasKey', ps.HasKey('n'), true);
check('PropertySet Size', ps.Size, 3);
check('PropertySet missing', ps['zz'], undefined);

const vs = new ValueSet();
vs['x'] = 3.5;
check('ValueSet double round-trip', vs['x'], 3.5);
check('ValueSet in operator', 'x' in vs, true);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
