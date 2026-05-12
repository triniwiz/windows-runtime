console.log("NativeScript Windows TestApp template booting...");
console.log("performance.now() =", performance.now());

const uri = new Windows.Foundation.Uri("https://nativescript.org/");
console.log("Sample API call, AbsoluteUri:", uri.AbsoluteUri);

function runRuntimeConformanceTests() {
	let passed = 0;
	let failed = 0;

	function expect(condition, message) {
		if (!condition) {
			throw new Error(message);
		}
	}

	function test(name, fn) {
		try {
			fn();
			passed += 1;
			console.log("[PASS]", name);
		} catch (error) {
			failed += 1;
			console.log("[FAIL]", name, "-", error && error.message ? error.message : error);
		}
	}

	// Class extension parity tests
	test("Class.extend exists", () => {
		expect(
			typeof Windows.Data.Json.JsonObject.extend === "function",
			"Windows.Data.Json.JsonObject.extend is not available"
		);
	});

	test("Class.extend supports Class.extend(name, overrides)", () => {
		const Extended = Windows.Data.Json.JsonObject.extend("TemplateNamedJsonObject", {
			ToString() {
				return "TemplateNamedJsonObject";
			},
		});
		const instance = new Extended();
		// Debug: inspect instance and prototype to see why override may not be applied
		console.log("[DEBUG] Named extend instance:", instance);
		try {
			console.log("[DEBUG] Named instance.ToString():", instance.ToString && instance.ToString());
		} catch (e) {
			console.log("[DEBUG] Named ToString() threw:", e && e.message ? e.message : e);
		}
		console.log("[DEBUG] Named instance.ToString typeof:", typeof instance.ToString);
		console.log("[DEBUG] Named prototype ToString typeof:", typeof Object.getPrototypeOf(instance).ToString);
		expect(instance.ToString() === "TemplateNamedJsonObject", "override ToString was not used");
	});

	test("Class.extend supports Class.extend(overrides)", () => {
		const Extended = Windows.Data.Json.JsonObject.extend({
			ToString() {
				return "TemplateUnnamedJsonObject";
			},
		});
		const instance = new Extended();
		// Debug: inspect instance and prototype to see why override may not be applied
		console.log("[DEBUG] Unnamed extend instance:", instance);
		try {
			console.log("[DEBUG] Unnamed instance.ToString():", instance.ToString && instance.ToString());
		} catch (e) {
			console.log("[DEBUG] Unnamed ToString() threw:", e && e.message ? e.message : e);
		}
		console.log("[DEBUG] Unnamed instance.ToString typeof:", typeof instance.ToString);
		console.log("[DEBUG] Unnamed prototype ToString typeof:", typeof Object.getPrototypeOf(instance).ToString);
		expect(instance.ToString() === "TemplateUnnamedJsonObject", "unnamed extend override failed");
	});

	// Interface implementation parity tests
	test("Single interface implementation new Interface({ ... })", () => {
		const impl = new Windows.Foundation.IStringable({
			ToString() {
				return "single-interface";
			},
		});
		expect(impl.ToString() === "single-interface", "single interface ToString implementation failed");
	});

	test("Multi-interface pattern Object.extend({ interfaces: [...] })", () => {
		expect(typeof Object.extend === "function", "Object.extend is not available");
		const Multi = Object.extend({
			interfaces: [Windows.Foundation.IStringable],
			ToString() {
				return "multi-interface";
			},
		});
		const impl = new Multi();
		expect(impl.ToString() === "multi-interface", "multi-interface implementation failed");
	});

	// Delegate / event helper semantics
	test("Delegate helper NSWinRT.asDelegate(function)", () => {
		expect(typeof NSWinRT.asDelegate === "function", "NSWinRT.asDelegate is not available");
		const fn = NSWinRT.asDelegate((value) => value + 1);
		expect(fn(41) === 42, "delegate helper did not preserve function behavior");
	});

	test("Delegate helper NSWinRT.asDelegate({ invoke })", () => {
		const delegate = NSWinRT.asDelegate({
			invoke(a, b) {
				return a + b;
			},
		});
		expect(delegate(20, 22) === 42, "object delegate invoke mapping failed");
	});

	test("Event helper NSWinRT.createEventEmitter add/remove/emit", () => {
		expect(typeof NSWinRT.createEventEmitter === "function", "NSWinRT.createEventEmitter is not available");
		const emitter = NSWinRT.createEventEmitter();
		let total = 0;
		const sub = emitter.add((value) => {
			total += value;
		});
		emitter.emit(40);
		emitter.emit(2);
		expect(total === 42, "event emitter did not notify listeners");
		sub.dispose();
		emitter.emit(10);
		expect(total === 42, "event emitter unsubscribe did not remove listener");
	});

	console.log(`[TEST SUMMARY] passed=${passed}, failed=${failed}`);
}

try {
	runRuntimeConformanceTests();
}catch (error) {
	console.log("[ERROR] Test execution threw an error:", error && error.message ? error.message : error);
}

try {
	const data = new Windows.Data.Json.JsonObject();
	console.dir("??", data);
}catch (error) {
	console.log("[ERROR] console.dir threw an error:", error && error.message ? error.message : error);
}

// Optional async sample:
// const dialog = new Windows.UI.Popups.MessageDialog("Hello from NativeScript Windows template");
// const dialog = new Windows.UI.Popups.MessageDialog("Hello from NativeScript Windows template");
// NSWinRT.toPromise(dialog.ShowAsync(), { timeoutMs: 10000 })
//   .then((result) => console.log("Dialog result:", result))
//   .catch((error) => console.log("Dialog error:", error));
