(function () {
    // Small demo: create/update/remove a composition border using runtime helpers
    // Falls back to warnings if the runtime bindings are not exposed to JS.

    function resolveCreateFn() {
        if (typeof NSWinRT !== 'undefined' && typeof NSWinRT.createCompositionBorder === 'function') return NSWinRT.createCompositionBorder;
        if (typeof runtime_create_border_instance === 'function') return runtime_create_border_instance;
        if (typeof NSWinRT !== 'undefined' && typeof NSWinRT.createBorder === 'function') return NSWinRT.createBorder;
        return null;
    }

    function resolveSetFn() {
        if (typeof runtime_set_border === 'function') return runtime_set_border;
        if (typeof NSWinRT !== 'undefined' && typeof NSWinRT.setBorder === 'function') return NSWinRT.setBorder;
        return null;
    }

    function resolveFreeFn() {
        if (typeof runtime_free_border_instance === 'function') return runtime_free_border_instance;
        if (typeof NSWinRT !== 'undefined' && typeof NSWinRT.freeBorder === 'function') return NSWinRT.freeBorder;
        return null;
    }

    function createJSBorder(element) {
        const createFn = resolveCreateFn();
        const setFn = resolveSetFn();
        const freeFn = resolveFreeFn();

        if (!createFn) {
            console.warn('Border demo: runtime_create_border_instance not available.');
            return null;
        }

        // If the runtime provides the new higher-level helper, use it and
        // return the proxy object directly (it auto-finalizes on GC).
        if (typeof NSWinRT !== 'undefined' && typeof NSWinRT.createCompositionBorder === 'function') {
            try {
                return NSWinRT.createCompositionBorder(element);
            } catch (e) {
                console.warn('createCompositionBorder failed:', e);
            }
        }

        // Many WinRT proxies expose an internal numeric handle as `__handle`.
        const elHandle = (element && typeof element === 'object' && typeof element.__handle === 'number')
            ? element.__handle
            : element;

        const id = createFn(elHandle);
        if (!id) {
            console.warn('Border demo: create returned falsy id');
            return null;
        }

        const holder = { id, element };

        // Install shared finalizer registry on window so GC will free the native instance.
        if (typeof FinalizationRegistry === 'function') {
            if (!window.__ns_border_finalizer) {
                window.__ns_border_finalizer = new FinalizationRegistry(function (id) {
                    try {
                        const f = resolveFreeFn();
                        if (f) f(id);
                    } catch (e) {}
                });
            }
            window.__ns_border_finalizer.register(holder, id);
        }

        holder.update = function (left, top, right, bottom, color, r_tl, r_tr, r_br, r_bl) {
            const f = resolveSetFn();
            if (!f) { console.warn('Border demo: runtime_set_border not available'); return; }
            f(id, left, top, right, bottom, color, r_tl, r_tr, r_br, r_bl);
        };

        holder.free = function () {
            const f = resolveFreeFn();
            if (f) f(id);
        };

        return holder;
    }

    function runDemo() {
        try {
            const window = Windows.UI.Xaml.Window.Current;
            const panel = new Windows.UI.Xaml.Controls.StackPanel();
            panel.Orientation = Windows.UI.Xaml.Controls.Orientation.Vertical;

            const sample = new Windows.UI.Xaml.Controls.Border();
            sample.Width = 320;
            sample.Height = 160;
            sample.Margin = new Windows.UI.Xaml.Thickness(20);
            sample.Background = new Windows.UI.Xaml.Media.SolidColorBrush(Windows.UI.Colors.AliceBlue);

            panel.Children.Append(sample);

            const status = new Windows.UI.Xaml.Controls.TextBlock();
            status.Text = 'Border demo: initializing...';
            status.Margin = new Windows.UI.Xaml.Thickness(20, 6, 20, 20);
            panel.Children.Append(status);

            window.Content = panel;
            window.Activate();

            const holder = createJSBorder(sample);
            if (!holder) {
                status.Text = 'Border demo: runtime bindings not available.';
                return;
            }

            status.Text = 'Border demo: created instance ' + holder.id;

            // initial border: 4px all sides, blue, radius 8
            holder.update(4, 4, 4, 4, 0xFF0078D7, 8, 8, 8, 8);

            // update after 2s
            setTimeout(function () {
                holder.update(8, 2, 8, 2, 0xFF00C853, 16, 8, 12, 4);
                status.Text = 'Border demo: updated';
            }, 2000);

            // drop after 6s so runtime finalizer/unloaded handlers perform cleanup
            setTimeout(function () {
                window.__demo_border_holder = null;
                status.Text = 'Border demo: dropped holder (runtime will clean up)';
            }, 6000);

            // keep a debug reference so you can inspect it in devtools
            window.__demo_border_holder = holder;
        } catch (e) {
            console.warn('Border demo failed:', e && e.message ? e.message : e);
        }
    }

    // Auto-run when loaded
    if (typeof document === 'undefined') {
        try { runDemo(); } catch (e) {}
    } else {
        document.addEventListener('DOMContentLoaded', runDemo);
    }
})();
