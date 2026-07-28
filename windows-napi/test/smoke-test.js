// Smoke test: load the .node addon, boot the runtime, run script, verify error plumbing.
const ns = require('../index.js');

console.log('exports:', Object.keys(ns));

console.log('init ->', ns.init(process.cwd()));

// Valid script: should execute with no error recorded.
ns.runScript('const x = 2 + 2; globalThis.__x = x;', 'ok.js');
console.log('lastError after ok script ->', ns.lastError());

// Throwing script: should populate lastError with message + stack.
ns.runScript('throw new Error("boom from napi bridge");', 'boom.js');
console.log('lastError after throw ->', ns.lastError());

ns.pumpTimers();
ns.deinit();
console.log('done');
