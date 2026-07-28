// Shared smoke app for the standalone hosts' script mode:
//   nativescript-windows packages/test-app.js
// Exercises WinRT sync calls, timers, and an awaited WinRT async operation; the process must
// print the lines in order and then exit on its own once the event loop goes idle.
'use strict';

console.log('app: start');

var obj = new Windows.Data.Json.JsonObject();
obj.SetNamedValue('n', Windows.Data.Json.JsonValue.CreateNumberValue(3));
console.log('app: json = ' + obj.Stringify());

setTimeout(function () {
  console.log('app: timeout fired');
  NSWinRT.toPromise(Windows.System.Threading.ThreadPool.RunAsync(function () {}))
    .then(function () {
      console.log('app: winrt async done');
    })
    .catch(function (e) {
      console.error('app: winrt async FAILED: ' + (e && e.message ? e.message : e));
    });
}, 30);
