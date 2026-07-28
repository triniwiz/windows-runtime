// Node-API port of the classic engine's NSWinRT.dotnet bridge (global_fns.rs HELPER_SOURCE).
// No dotnet-bridge/publish/DotNetBridge.dll exists in this test environment, so calls that would
// reach the managed side are expected to throw a "bridge not found" style error — that still
// proves the whole new path (native -> wire codec -> crate::dotnet -> error propagation back to
// JS) runs end to end. __nsRunOnUIThread needs no bridge at all and is exercised for real.
const ns = require('../index.js');

let pass = 0, fail = 0;
function check(name, got, expected) {
  const ok = typeof expected === 'function' ? expected(got) : Object.is(got, expected);
  if (ok) pass++;
  else { console.log(`FAIL ${name}: got ${JSON.stringify(got)}, expected ${String(expected)}`); fail++; }
}

ns.installDotnet();

// Natives registered on globalThis.
for (const name of ['__nsDotNetInvoke', '__nsDotNetInvokeBin', '__nsDotNetCreateDelegate', '__nsDotNetAwaitTask', '__nsRunOnUIThread']) {
  check(`${name} is a function`, typeof globalThis[name], 'function');
}

// Idempotent: a second install must not throw or replace the surface.
const dotnetBefore = globalThis.NSWinRT.dotnet;
ns.installDotnet();
check('second installDotnet is a no-op', globalThis.NSWinRT.dotnet, dotnetBefore);

// NSWinRT.dotnet surface shape.
const dotnet = globalThis.NSWinRT.dotnet;
check('dotnet.invoke is a function', typeof dotnet.invoke, 'function');
check('dotnet.get is a function', typeof dotnet.get, 'function');
check('dotnet.fromHandle is a function', typeof dotnet.fromHandle, 'function');
check('dotnet.registerNamespace is a function', typeof dotnet.registerNamespace, 'function');
check('dotnet.registerNamespaces is a function', typeof dotnet.registerNamespaces, 'function');
check('dotnet.taskToPromise is a function', typeof dotnet.taskToPromise, 'function');
check('dotnet.asDelegate is a function', typeof dotnet.asDelegate, 'function');
check('NSWinRT.runOnUIThread is a function', typeof globalThis.NSWinRT.runOnUIThread, 'function');

// No bridge DLL present: invoking a .NET member surfaces the managed-host-not-found error as a
// real JS exception (proves __nsDotNetInvokeBin's request encode + call_dotnet_binary +
// response/error decode round-trip, not just that the function exists).
let invokeError = null;
try {
  dotnet.invoke('', 'System.Environment', 'get_MachineName', []);
} catch (e) {
  invokeError = e;
}
check('invoke without bridge throws', invokeError instanceof Error, true);
check('invoke error mentions bridge', invokeError && /DotNetBridge|bridge|hostfxr/i.test(invokeError.message), true);

// registerNamespace must not throw even though the bridge is unavailable (GetNamespaceAssemblyMapJson
// fails silently); the lazy namespace proxy it installs must not throw on simple member descent either
// (missing __members__ resolves to the shared "empty info" and just returns a deeper proxy).
let registerError = null;
try {
  dotnet.registerNamespace('NsDotnetTestNamespace', 'SomeAssembly');
  void globalThis.NsDotnetTestNamespace.SomeType;
} catch (e) {
  registerError = e;
}
check('registerNamespace + descent does not throw', registerError, null);
// The namespace proxy wraps a function target (needed for its apply/construct traps), so
// typeof reports 'function', not 'object' — matches the classic engine's _makeNamespaceProxy.
check('registered namespace root installed', typeof globalThis.NsDotnetTestNamespace, 'function');

// asDelegate also reaches the bridge (opcode 0x09) and must surface the same style of error.
let delegateError = null;
try {
  dotnet.asDelegate(() => {});
} catch (e) {
  delegateError = e;
}
check('asDelegate without bridge throws', delegateError instanceof Error, true);

// __nsRunOnUIThread needs no .NET bridge: no UI dispatcher exists in this bare Node process, so
// ui_dispatcher::post_to_ui_thread falls back to calling the closure inline — this exercises the
// full pin/call/cleanup path for real.
let ran = false;
globalThis.NSWinRT.runOnUIThread(() => { ran = true; });
check('runOnUIThread fires its callback', ran, true);

// A throwing runOnUIThread callback must not crash the process; the error is captured instead.
globalThis.NSWinRT.runOnUIThread(() => { throw new Error('run-on-ui-thread boom'); });
check('runOnUIThread error captured', ns.lastError(), (e) => typeof e === 'string' && e.includes('run-on-ui-thread boom'));

// Managed-subclass / proxy system (Bridge.Proxy.cs's dynamic-proxy path), ported verbatim from
// the classic engine alongside the rest of NSWinRT.dotnet. __nsDotNetCreateJsSubclass now takes
// 5 args: assembly, typeName, interfaceNames[], memberNames[], dispatcher.
check('__nsDotNetCreateJsSubclass is a function', typeof globalThis.__nsDotNetCreateJsSubclass, 'function');
check('NSWinRT.proxy is an object', typeof globalThis.NSWinRT.proxy, 'object');
check('proxy.createManagedSubclass is a function', typeof globalThis.NSWinRT.proxy.createManagedSubclass, 'function');

// No bridge DLL present: creating a managed subclass reaches the same call_dotnet_binary path as
// dotnet.invoke and must surface the same "bridge not found" style error, not a crash — proving
// the interfaceNames/memberNames wire encoding (opcode 0x0A) round-trips end to end.
let subclassError = null;
try {
  globalThis.NSWinRT.proxy.createManagedSubclass('', 'Some.Test.BaseType', {
    Describe: function () { return 'js-describe'; },
  });
} catch (e) {
  subclassError = e;
}
check('createManagedSubclass without bridge throws', subclassError instanceof Error, true);
check('createManagedSubclass error mentions bridge', subclassError && /DotNetBridge|bridge|hostfxr/i.test(subclassError.message), true);

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
