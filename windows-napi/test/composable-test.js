// E2E: composable (non-sealed) classes under napi.
//
// Every activatable non-sealed WinRT class lives in Windows.UI.Xaml, and XAML activation
// requires a XAML-initialized thread — headless (Node, standalone engine hosts, and the classic
// runtime alike) the factory returns RPC_E_WRONG_THREAD (0x8001010E). What IS verifiable
// headless:
//   1. the composable ctor path executes end-to-end (arity match → MethodCall composition
//      branch with null outer/inner → clean HRESULT surfaced as a catchable JS error, no
//      crash/stack corruption),
//   2. composable class TREES work: Windows.UI.Composition objects (non-sealed bases like
//      Visual/ContainerVisual/CompositionBrush) wrap, dispatch inherited members, and
//      round-trip as arguments,
//   3. in a XAML-capable host the same ctors succeed (the success arm below).
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

const W = ns.getNamespace('Windows');
const WRONG_THREAD = /different thread|0x8001010E/i;

// Guard the premise: these classes are really non-sealed (composable), so `new` takes the
// composition (null-outer) branch, not the plain-activation ABI.
check('FontFamily is composable', ns.classIsSealed('Windows.UI.Xaml.Media.FontFamily'), false);
check('RoutedEventArgs is composable', ns.classIsSealed('Windows.UI.Xaml.RoutedEventArgs'), false);
check('PropertyChangedEventArgs is composable', ns.classIsSealed('Windows.UI.Xaml.Data.PropertyChangedEventArgs'), false);
check('sanity: JsonObject is sealed', ns.classIsSealed('Windows.Data.Json.JsonObject'), true);

// Attempt a parameterized composable ctor. Success (XAML-capable env) or the specific
// wrong-thread HRESULT as a clean JS error (headless) both pass; anything else — crash, junk,
// a different error — fails.
function composableCtor(name, make, onSuccess) {
  try {
    const obj = make();
    check(name + ' (constructed)', onSuccess(obj), true);
  } catch (e) {
    check(name + ' (headless: clean wrong-thread error)', WRONG_THREAD.test(e.message), true);
  }
}
composableCtor('FontFamily 1-arg',
  () => new W.UI.Xaml.Media.FontFamily('Segoe UI'),
  (ff) => ff.Source === 'Segoe UI');
composableCtor('FontFamily CSS list',
  () => new W.UI.Xaml.Media.FontFamily('Arial, Helvetica, sans-serif'),
  (ff) => typeof ff.Source === 'string');
composableCtor('PropertyChangedEventArgs 1-arg',
  () => new W.UI.Xaml.Data.PropertyChangedEventArgs('MyProp'),
  (p) => p.PropertyName === 'MyProp');
composableCtor('RoutedEventArgs 0-arg',
  () => new W.UI.Xaml.RoutedEventArgs(),
  (e) => e.__typeName__ === 'Windows.UI.Xaml.RoutedEventArgs');

// Composable class trees headless: Windows.UI.Composition. Compositor needs a DispatcherQueue
// on the calling thread — ensure_winrt_initialized provides one.
const c = new W.UI.Composition.Compositor();
check('Compositor constructs', typeof c, 'object');
const v = c.CreateSpriteVisual();
check('SpriteVisual wraps by runtime class', v.__typeName__, 'Windows.UI.Composition.SpriteVisual');
v.Opacity = 0.5; // property declared on the non-sealed base class Visual
check('inherited property (Visual.Opacity) round-trip', v.Opacity, 0.5);
const brush = c.CreateColorBrush();
v.Brush = brush; // CompositionBrush-typed property takes a subclass instance
check('composable-typed property set/get', v.Brush.__typeName__, 'Windows.UI.Composition.CompositionColorBrush');
const child = c.CreateSpriteVisual();
v.Children.InsertAtTop(child); // ContainerVisual member on a SpriteVisual + visual as argument
check('inherited collection member + visual as argument', v.Children.Count, 1);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
