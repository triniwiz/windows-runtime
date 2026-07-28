/* Minimal embedding shim over quickjs-ng, used to prove the engine compiles under MSVC (via
 * the `cc` crate) and runs JS from Rust — the first gate for a standalone QuickJS host.
 * Avoids exposing JSValue-by-value across the Rust FFI boundary (tagged union, tricky
 * ABI) by keeping all JSValue handling inside C and returning plain int / C-string. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "quickjs.h"

/* Evaluate `code` as a global script and return its Int32 coercion, or `err` on exception. */
int32_t qjs_eval_int(const char *code, int32_t err) {
    JSRuntime *rt = JS_NewRuntime();
    if (!rt) return err;
    JSContext *ctx = JS_NewContext(rt);
    if (!ctx) { JS_FreeRuntime(rt); return err; }

    JSValue val = JS_Eval(ctx, code, strlen(code), "<eval>", JS_EVAL_TYPE_GLOBAL);
    int32_t result = err;
    if (!JS_IsException(val)) {
        JS_ToInt32(ctx, &result, val);
    }
    JS_FreeValue(ctx, val);
    JS_FreeContext(ctx);
    JS_FreeRuntime(rt);
    return result;
}

/* Evaluate `code` and return its string coercion as a malloc'd C string (caller frees via
 * qjs_free), or NULL on exception. Proves string round-trip through the engine. */
char *qjs_eval_str(const char *code) {
    JSRuntime *rt = JS_NewRuntime();
    if (!rt) return NULL;
    JSContext *ctx = JS_NewContext(rt);
    if (!ctx) { JS_FreeRuntime(rt); return NULL; }

    JSValue val = JS_Eval(ctx, code, strlen(code), "<eval>", JS_EVAL_TYPE_GLOBAL);
    char *out = NULL;
    if (!JS_IsException(val)) {
        const char *s = JS_ToCString(ctx, val);
        if (s) {
            size_t n = strlen(s) + 1;
            out = (char *)malloc(n);
            if (out) memcpy(out, s, n);
            JS_FreeCString(ctx, s);
        }
    }
    JS_FreeValue(ctx, val);
    JS_FreeContext(ctx);
    JS_FreeRuntime(rt);
    return out;
}

void qjs_free(char *p) { free(p); }
