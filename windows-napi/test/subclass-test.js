// E2E: native subclassing — `class Sub extends WinRTClass` — on both object models:
// host objects (JsonObject/Uri: super() re-links the instance to Sub.prototype, which chains
// to the shared class prototype) and Proxy-path instances (StringMap: the construct trap
// honors newTarget, subclass members resolve through the target's prototype chain).
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

const W = ns.getNamespace('Windows');
const { JsonObject, JsonValue } = W.Data.Json;
const { Uri } = W.Foundation;
const { StringMap } = W.Foundation.Collections;

// Host-object path: subclass with own field, extra method, override calling super.
class MyObj extends JsonObject {
  constructor() {
    super();
    this.tag = 42;
  }
  custom() { return 'custom:' + this.Stringify(); }
  Stringify() { return 'wrapped[' + super.Stringify() + ']'; }
}
const m = new MyObj();
check('instanceof subclass', m instanceof MyObj, true);
check('instanceof WinRT base', m instanceof JsonObject, true);
check('own field from constructor', m.tag, 42);
m.SetNamedValue('k', JsonValue.CreateStringValue('v'));
check('inherited WinRT method + override + super', m.Stringify(), 'wrapped[{"k":"v"}]');
check('subclass method sees WinRT members', m.custom(), 'custom:wrapped[{"k":"v"}]');
check('inherited WinRT getter', m.GetNamedString('k'), 'v');

// Direct `new` of the base is unaffected by subclass construction.
const plain = new JsonObject();
check('plain instance keeps shared prototype', Object.getPrototypeOf(plain), JsonObject.prototype);
check('plain not instanceof subclass', plain instanceof MyObj, false);

// Subclass instances marshal as WinRT arguments; the identity cache returns the same JS object.
const outer = new JsonObject();
outer.SetNamedValue('sub', m);
const back = outer.GetNamedObject('sub');
check('subclass as WinRT argument round-trips', back === m, true);
check('round-tripped override intact', back.Stringify(), 'wrapped[{"k":"v"}]');

// Constructor arguments flow through super(); accessors defined on the subclass work.
class MyUri extends Uri {
  constructor(u) { super(u); }
  get host2() { return this.Host + '!'; }
}
const u = new MyUri('https://example.com/x');
check('super(args) parameterized ctor', u.AbsoluteUri, 'https://example.com/x');
check('subclass getter over WinRT property', u.host2, 'example.com!');
check('instanceof Uri', u instanceof Uri, true);

// Two-level inheritance.
class GrandObj extends MyObj {
  custom() { return 'grand:' + super.custom(); }
}
const g = new GrandObj();
check('grandchild instanceof chain', g instanceof GrandObj && g instanceof MyObj && g instanceof JsonObject, true);
check('grandchild override chain', g.custom(), 'grand:custom:wrapped[{}]');

// Proxy-path subclass (StringMap stays a Proxy for keyed sugar).
class MyMap extends StringMap {
  describe() { return 'size=' + this.Size; }
}
const pm = new MyMap();
pm['a'] = '1';
check('proxy subclass method', pm.describe(), 'size=1');
check('proxy subclass instanceof subclass', pm instanceof MyMap, true);
check('proxy subclass instanceof WinRT base', pm instanceof StringMap, true);
check('keyed sugar still works on subclass', pm['a'], '1');

// Expandos on plain (non-map) instances survive via the Proxy target.
const feed = new W.Web.Syndication.SyndicationFeed();
feed.customNote = 'hi';
check('expando on host instance', feed.customNote, 'hi');

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
