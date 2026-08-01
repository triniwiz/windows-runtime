// Shared WinRT-interop microbenchmark. Run identically on the classic V8 runtime (via playground,
// PLAYGROUND_SCRIPT_PATH) and on each napi standalone host (embedded via include_str!). Measures the
// interop path — where classic (rusty_v8 seam) and napi (Proxy traps + napi marshaling) differ.
// Emits tab-separated "BENCH\t<name>\t<ms>\t<ops/sec>" lines for easy parsing.
(function () {
  var N = 20000;     // timed iterations per op
  var WARM = 2000;   // warmup iterations (let V8/JSC JIT settle)
  var now = (typeof performance !== 'undefined' && performance.now)
    ? function () { return performance.now(); }
    : function () { return __time(); };

  // Accumulate results and RETURN them (the script's completion value). Hosts print the returned
  // string via Rust stdout — reliable under redirection, unlike napi console.log (WriteConsoleW).
  var out = '';
  function bench(name, fn) {
    var line;
    try {
      for (var i = 0; i < WARM; i++) fn(i);
      var t0 = now();
      for (var i = 0; i < N; i++) fn(i);
      var ms = now() - t0;
      var ops = Math.round(N / (ms / 1000));
      line = 'BENCH\t' + name + '\t' + ms.toFixed(2) + '\t' + ops;
    } catch (e) {
      line = 'BENCH\t' + name + '\tERROR\t' + e;
    }
    out += line + '\n';
    console.log(line); // for the classic runtime (playground), whose console.log is captured
  }

  var JV = Windows.Data.Json.JsonValue;
  var JO = Windows.Data.Json.JsonObject;

  // static method call → instance proxy → instance method + number marshaling both ways
  bench('static_call_getnumber', function (i) { return JV.CreateNumberValue(i).GetNumber(); });
  // activation (parameterless construction through the activation factory)
  bench('construct_jsonobject', function () { return new JO(); });
  // instance method + proxy-as-argument marshaling + property read-back
  bench('set_get_named', function (i) {
    var o = new JO(); o.SetNamedValue('k', JV.CreateNumberValue(i)); return o.GetNamedNumber('k');
  });
  // string marshaling round-trip (JS string → HSTRING → JS string)
  bench('string_roundtrip', function (i) { return JV.CreateStringValue('x' + i).GetString(); });
  // pure member resolution (namespace/ctor get-trap + metadata lookup, no invocation)
  bench('member_resolve', function () { return JV.CreateNumberValue; });

  console.log('BENCH_DONE');
  out += 'BENCH_DONE\n';
  return out;
})();
