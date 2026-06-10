use runtime::Runtime;

#[test]
fn json_tryparse_returns_out_value() {
    let mut rt = Runtime::new(".");

    // Skip if JsonValue.TryParse isn't available (no package identity).
    let avail = rt.eval_script_to_string(
        "typeof Windows !== 'undefined' && Windows.Data && Windows.Data.Json && Windows.Data.Json.JsonValue && typeof Windows.Data.Json.JsonValue.TryParse === 'function'",
    ).unwrap_or_else(|| "false".to_string());
    if avail.trim() != "true" {
        eprintln!("SKIP: JsonValue.TryParse not available (no package identity)");
        return;
    }

    // Call TryParse on a JSON string literal. Expect an array [true, JsonValue].
    let res = rt.eval_script_to_string(r#"
        (function(){
            try {
                var r = Windows.Data.Json.JsonValue.TryParse('"hello"');
                if (!Array.isArray(r)) return 'not-array';
                if (!r[0]) return 'parse-failed';
                return r[1].GetString();
            } catch(e) {
                try { return 'exception:' + (e && e.message ? e.message : e.toString()); } catch(e2) { return 'exception:unknown'; }
            }
        })()
    "#).unwrap_or_else(|| "<eval-failed>".to_string());

    assert_eq!(
        res.trim(),
        "hello",
        "TryParse out-param roundtrip failed: got {res:?}"
    );
}
