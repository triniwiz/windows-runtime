use std::fs;
use std::path::Path;
use v8::{ContextScope, HandleScope};

/// Copy `source_path` into `dest_path`, creating parent directories as needed.
pub fn copy_file(source_path: &str, dest_path: &str) -> Result<(), String> {
    let dest = Path::new(dest_path);
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create destination directory: {e}"))?;
        }
    }
    fs::copy(source_path, dest_path)
        .map_err(|e| format!("Failed to copy '{source_path}' -> '{dest_path}': {e}"))?;
    Ok(())
}

/// Installs `globalThis.NSWinRT.LiveSync` — the JS surface for live synchronisation.
///
/// LiveSync sits on top of the existing HMR module-reload primitives.  The host
/// tooling (e.g. a CLI watcher) calls into this API to push changed files into the
/// running app without a full restart:
///
/// ```js
/// // Copy a source file into the app bundle and reload it:
/// NSWinRT.LiveSync.sync('/build/app/main.js', 'C:/app/main.js');
///
/// // Just reload an already-present file (no copy needed):
/// NSWinRT.LiveSync.reload('main.js');
///
/// // Full app reset — clears the entire module cache:
/// NSWinRT.LiveSync.reset();
/// ```
pub fn install_livesync_support(scope: &mut ContextScope<HandleScope>) {
    let source = r#"
    (function () {
        if (!globalThis.NSWinRT) {
            return;
        }

        if (globalThis.NSWinRT.LiveSync) {
            return;
        }

        function resolveModulePath(modulePath, parentPath) {
            if (typeof globalThis.__nsResolveModulePath !== 'function') {
                throw new Error('LiveSync requires __nsResolveModulePath host hook');
            }
            return globalThis.__nsResolveModulePath(
                String(modulePath || ''),
                parentPath ? String(parentPath) : '',
                globalThis.__nsAppRoot || ''
            );
        }

        function invalidateEntry(resolvedPath) {
            if (typeof globalThis.__nsInvalidateModuleCacheEntry === 'function') {
                globalThis.__nsInvalidateModuleCacheEntry(String(resolvedPath || ''));
            }
        }

        function evalModule(resolvedPath) {
            if (typeof globalThis.__nsReadTextFile !== 'function') {
                throw new Error('LiveSync requires __nsReadTextFile host hook');
            }
            if (typeof globalThis.__nsEvalAsModule !== 'function') {
                throw new Error('LiveSync requires __nsEvalAsModule host hook');
            }
            var source = globalThis.__nsReadTextFile(resolvedPath);
            return globalThis.__nsEvalAsModule(source, resolvedPath);
        }

        /**
         * Copy a built file from `sourcePath` into `destPath` inside the running
         * app bundle, then invalidate and re-execute the destination module.
         *
         * `sourcePath` and `destPath` are absolute filesystem paths. The Rust host
         * performs the copy; JS performs the module-cache invalidation and reload.
         */
        function sync(sourcePath, destPath) {
            if (typeof globalThis.__nsLiveSyncCopyFile !== 'function') {
                throw new Error('LiveSync.sync requires the __nsLiveSyncCopyFile host hook');
            }

            var src = String(sourcePath || '');
            var dst = String(destPath || '');
            if (!src || !dst) {
                throw new Error('LiveSync.sync(sourcePath, destPath) requires two non-empty path strings');
            }

            globalThis.__nsLiveSyncCopyFile(src, dst);

            var resolved = resolveModulePath(dst, '');
            if (resolved) {
                invalidateEntry(resolved);
                evalModule(resolved);
            }
        }

        /**
         * Reload a module that is already present in the app bundle at `modulePath`
         * (relative or absolute).  No file copy is performed.
         */
        function reload(modulePath) {
            var resolved = resolveModulePath(String(modulePath || ''), '');
            if (!resolved) {
                throw new Error('LiveSync.reload: unable to resolve module path: ' + modulePath);
            }
            invalidateEntry(resolved);
            return evalModule(resolved);
        }

        /**
         * Clear the entire module cache and re-evaluate the app entry point (if
         * `__nsAppRoot` is set).  Use this for a full hot-restart.
         */
        function reset() {
            if (typeof globalThis.__nsClearModuleCache === 'function') {
                globalThis.__nsClearModuleCache();
            }
        }

        globalThis.NSWinRT.LiveSync = {
            sync:   sync,
            reload: reload,
            reset:  reset,
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
