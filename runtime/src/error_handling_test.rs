use crate::Runtime;

#[test]
fn test_error_thrown_in_js_is_caught_and_rethrown() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        throw new Error("Test error from JavaScript");
    "#;

    runtime.run_script(script, "test_error_thrown.js");
}

#[test]
fn test_syntax_error_is_caught() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        this is not valid javascript !!!
    "#;

    runtime.run_script(script, "test_syntax_error.js");
}

#[test]
fn test_type_error_is_caught() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        let x = 123;
        x();  // This will throw TypeError: x is not a function
    "#;

    runtime.run_script(script, "test_type_error.js");
}

#[test]
fn test_null_access_error_is_caught() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        let x = null;
        x.someProperty;  // This will throw TypeError
    "#;

    runtime.run_script(script, "test_null_access_error.js");
}

#[test]
fn test_custom_error_with_message_and_stack() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        function testFunction() {
            throw new Error("Custom error in testFunction");
        }

        function outerFunction() {
            testFunction();
        }

        outerFunction();
    "#;

    runtime.run_script(script, "test_custom_error.js");
}

#[test]
fn test_error_in_async_context() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        Promise.reject(new Error("Promise rejection"));
    "#;

    runtime.run_script(script, "test_error_async.js");
}

#[test]
fn test_multiple_sequential_errors() {
    let mut runtime = Runtime::new(".");

    let script1 = r#"throw new Error("First error");"#;
    runtime.run_script(script1, "test_sequential_error_1.js");

    let script2 = r#"throw new Error("Second error");"#;
    runtime.run_script(script2, "test_sequential_error_2.js");

    let script3 = r#"console.log("After errors");"#;
    runtime.run_script(script3, "test_sequential_after_errors.js");
}

#[test]
fn test_error_in_module_import() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        export const value = (function() {
            throw new Error("Error during module initialization");
        })();
    "#;

    runtime.run_script(script, "test_error_module_import.js");
}

#[test]
fn test_nested_try_catch_with_rethrow() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        try {
            try {
                throw new Error("Inner error");
            } catch (e) {
                throw new Error("Rethrown: " + e.message);
            }
        } catch (e) {
            throw e;
        }
    "#;

    runtime.run_script(script, "test_nested_try_catch.js");
}

#[test]
fn test_error_doesnt_panic_runtime() {
    let mut runtime = Runtime::new(".");

    for i in 0..5 {
        let script = format!(r#"throw new Error("Error iteration {}");"#, i);
        runtime.run_script(&script, "test_error_stability.js");
    }
}

#[test]
fn test_error_with_empty_message() {
    let mut runtime = Runtime::new(".");

    let script = r#"throw new Error("");"#;
    runtime.run_script(script, "test_error_empty_message.js");
}

#[test]
fn test_non_error_throw() {
    let mut runtime = Runtime::new(".");

    let script = r#"throw "string error";"#;
    runtime.run_script(script, "test_non_error_throw.js");
}

#[test]
fn test_stack_trace_captures_function_names() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        function levelThree() {
            throw new Error("Error at level 3");
        }

        function levelTwo() {
            levelThree();
        }

        function levelOne() {
            levelTwo();
        }

        levelOne();
    "#;

    runtime.run_script(script, "test_stack_function_names.js");
}

#[test]
fn test_stack_trace_with_arrow_functions() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        const arrowFunc = () => {
            throw new Error("Error in arrow function");
        };

        const wrapper = () => {
            arrowFunc();
        };

        wrapper();
    "#;

    runtime.run_script(script, "test_stack_arrow_functions.js");
}

#[test]
fn test_stack_trace_with_anonymous_functions() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        function namedFunction() {
            (function() {
                throw new Error("Error in anonymous function");
            })();
        }

        namedFunction();
    "#;

    runtime.run_script(script, "test_stack_anonymous_functions.js");
}

#[test]
fn test_stack_trace_with_class_methods() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        class TestClass {
            method1() {
                this.method2();
            }

            method2() {
                throw new Error("Error in class method");
            }
        }

        const instance = new TestClass();
        instance.method1();
    "#;

    runtime.run_script(script, "test_stack_class_methods.js");
}

#[test]
fn test_stack_trace_with_deep_recursion() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        function recurse(n) {
            if (n === 0) {
                throw new Error("Recursion base case");
            }
            recurse(n - 1);
        }

        recurse(3);
    "#;

    runtime.run_script(script, "test_stack_deep_recursion.js");
}

#[test]
fn test_stack_trace_with_object_methods() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        const obj = {
            methodA: function() {
                this.methodB();
            },
            methodB: function() {
                throw new Error("Error in object method");
            }
        };

        obj.methodA();
    "#;

    runtime.run_script(script, "test_stack_object_methods.js");
}

#[test]
fn test_stack_trace_with_callback_chain() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        function executeCallback(cb) {
            cb();
        }

        function firstCallback() {
            executeCallback(secondCallback);
        }

        function secondCallback() {
            throw new Error("Error in callback chain");
        }

        firstCallback();
    "#;

    runtime.run_script(script, "test_stack_callback_chain.js");
}

#[test]
fn test_stack_trace_preserves_context_across_try_catch() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        function innerFunction() {
            throw new Error("Original error");
        }

        function middleFunction() {
            try {
                innerFunction();
            } catch (e) {
                throw new Error("Rethrown: " + e.message);
            }
        }

        function outerFunction() {
            middleFunction();
        }

        outerFunction();
    "#;

    runtime.run_script(script, "test_stack_try_catch_context.js");
}

#[test]
fn test_error_location_with_inline_throw() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        function lineCounterFunction() {
            const x = 1;
            const y = 2;
            const z = 3;
            throw new Error("Error at specific line");
        }

        lineCounterFunction();
    "#;

    runtime.run_script(script, "test_error_location.js");
}

#[test]
fn test_stack_trace_with_nested_error_constructors() {
    let mut runtime = Runtime::new(".");

    let script = r#"
        function createError() {
            return new Error("Error created in function");
        }

        function throwError() {
            const err = createError();
            throw err;
        }

        throwError();
    "#;

    runtime.run_script(script, "test_stack_nested_constructors.js");
}

#[test]
fn test_multiple_errors_show_different_stacks() {
    let mut runtime = Runtime::new(".");

    let script1 = r#"
        function first() {
            throw new Error("First error");
        }
        first();
    "#;
    runtime.run_script(script1, "test_multi_stack_first.js");

    let script2 = r#"
        function second() {
            function nested() {
                throw new Error("Second error");
            }
            nested();
        }
        second();
    "#;
    runtime.run_script(script2, "test_multi_stack_second.js");
}
