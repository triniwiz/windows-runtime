use v8::{ContextScope, HandleScope};

pub fn install_hmr_support(scope: &mut ContextScope<HandleScope>) {
    let source = r#"
    (function () {
        if (!globalThis.NSWinRT) {
            return;
        }

        if (globalThis.NSWinRT.HMR) {
            return;
        }

        function readSourceFromPath(modulePath) {
            if (typeof globalThis.__nsReadTextFile !== 'function') {
                throw new Error('HMR requires __nsReadTextFile host hook');
            }
            return globalThis.__nsReadTextFile(String(modulePath || ''));
        }

        function resolvePath(specifier, parentPath) {
            if (typeof globalThis.__nsResolveModulePath !== 'function') {
                throw new Error('HMR requires __nsResolveModulePath host hook');
            }
            return globalThis.__nsResolveModulePath(
                String(specifier || ''),
                parentPath ? String(parentPath) : '',
                globalThis.__nsAppRoot || ''
            );
        }

        function invalidate(modulePath) {
            if (typeof globalThis.__nsInvalidateModuleCacheEntry === 'function') {
                globalThis.__nsInvalidateModuleCacheEntry(String(modulePath || ''));
            }
        }

        function applyFromSource(modulePath, source) {
            var resolved = resolvePath(modulePath, '');
            if (!resolved) {
                throw new Error('Unable to resolve module for HMR: ' + modulePath);
            }

            invalidate(resolved);

            if (typeof globalThis.__nsEvalAsModule !== 'function') {
                throw new Error('HMR requires __nsEvalAsModule runtime helper');
            }

            return globalThis.__nsEvalAsModule(String(source || ''), resolved);
        }

        function applyFromFile(modulePath) {
            var resolved = resolvePath(modulePath, '');
            if (!resolved) {
                throw new Error('Unable to resolve module for HMR: ' + modulePath);
            }
            var source = readSourceFromPath(resolved);
            return applyFromSource(resolved, source);
        }

        function clear() {
            if (typeof globalThis.__nsClearModuleCache === 'function') {
                globalThis.__nsClearModuleCache();
            }
        }

        globalThis.NSWinRT.HMR = {
            resolvePath: resolvePath,
            invalidate: invalidate,
            applyFromSource: applyFromSource,
            applyFromFile: applyFromFile,
            clear: clear
        };
    })();
    "#;

    let Some(source) = v8::String::new(scope, source) else {
        return;
    };

    if let Some(script) = v8::Script::compile(scope, source, None) {
        script.run(scope);
    }
}
