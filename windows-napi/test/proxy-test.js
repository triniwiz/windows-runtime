// Proxy test: a native-backed JS Proxy with Rust get/set/has traps.
const ns = require('../index.js');

const obj = ns.makeNativeProxy();

console.log('has foo (before):', 'foo' in obj);   // has trap -> false
obj.foo = 42;                                       // set trap -> native store
console.log('has foo (after):', 'foo' in obj);     // has trap -> true
console.log('get foo:', obj.foo);                   // get trap -> 42
obj.bar = 7;
console.log('get bar:', obj.bar);                   // -> 7
console.log('get missing:', obj.missing);           // get trap -> undefined
console.log('has missing:', 'missing' in obj);      // -> false

// Round-trip sanity: mutate then re-read through the native store.
obj.foo = obj.foo + 100;
console.log('get foo (after +100):', obj.foo);      // -> 142
