use v8::{ContextScope, HandleScope};

/// Installs `MessagePort`, `MessageChannel`, and the internal
/// `globalThis.__nsMessagePortDispatch` helper used by the Worker fallback path.
pub fn install_message_port_runtime(scope: &mut ContextScope<HandleScope>) {
    let source = r#"
    (function () {
        if (typeof globalThis.MessagePort === 'function' && typeof globalThis.MessageChannel === 'function') {
            return;
        }

        var __portState = new WeakMap();

        function __createMessageEvent(target, data) {
            return {
                type: 'message',
                data: data,
                target: target,
                currentTarget: target,
                ports: []
            };
        }

        function __dispatchMessage(targetPort, data) {
            var state = __portState.get(targetPort);
            if (!state || state.closed) {
                return;
            }

            var schedule = typeof globalThis.queueMicrotask === 'function'
                ? globalThis.queueMicrotask.bind(globalThis)
                : function (cb) { Promise.resolve().then(cb); };

            schedule(function () {
                if (state.closed) {
                    return;
                }

                var evt = __createMessageEvent(targetPort, data);

                if (typeof state.onmessage === 'function') {
                    try {
                        state.onmessage.call(targetPort, evt);
                    } catch (err) {
                        if (typeof globalThis.setTimeout === 'function') {
                            globalThis.setTimeout(function () { throw err; }, 0);
                        } else {
                            throw err;
                        }
                    }
                }

                state.listeners.slice().forEach(function (listener) {
                    try {
                        listener.call(targetPort, evt);
                    } catch (err) {
                        if (typeof globalThis.setTimeout === 'function') {
                            globalThis.setTimeout(function () { throw err; }, 0);
                        } else {
                            throw err;
                        }
                    }
                });
            });
        }

        function MessagePort() {
            throw new TypeError('Illegal constructor');
        }

        MessagePort.prototype.postMessage = function (data) {
            var state = __portState.get(this);
            if (!state || state.closed || !state.peer || state.peer.closed) {
                return;
            }
            __dispatchMessage(state.peer.owner, data);
        };

        MessagePort.prototype.start = function () {
            // Ports are active immediately; start() is a no-op.
        };

        MessagePort.prototype.close = function () {
            var state = __portState.get(this);
            if (state) {
                state.closed = true;
            }
        };

        MessagePort.prototype.addEventListener = function (type, listener) {
            if (type !== 'message' || typeof listener !== 'function') {
                return;
            }
            var state = __portState.get(this);
            if (!state || state.closed) {
                return;
            }
            if (state.listeners.indexOf(listener) < 0) {
                state.listeners.push(listener);
            }
        };

        MessagePort.prototype.removeEventListener = function (type, listener) {
            if (type !== 'message' || typeof listener !== 'function') {
                return;
            }
            var state = __portState.get(this);
            if (!state) {
                return;
            }
            var idx = state.listeners.indexOf(listener);
            if (idx >= 0) {
                state.listeners.splice(idx, 1);
            }
        };

        Object.defineProperty(MessagePort.prototype, 'onmessage', {
            get: function () {
                var state = __portState.get(this);
                return state ? state.onmessage : null;
            },
            set: function (handler) {
                var state = __portState.get(this);
                if (state) {
                    state.onmessage = typeof handler === 'function' ? handler : null;
                }
            },
            enumerable: true,
            configurable: true
        });

        function __createPortPair() {
            var a = Object.create(MessagePort.prototype);
            var b = Object.create(MessagePort.prototype);

            var aState = { owner: a, peer: null, closed: false, onmessage: null, listeners: [] };
            var bState = { owner: b, peer: aState, closed: false, onmessage: null, listeners: [] };
            aState.peer = bState;

            __portState.set(a, aState);
            __portState.set(b, bState);

            return [a, b];
        }

        function MessageChannel() {
            var ports = __createPortPair();
            this.port1 = ports[0];
            this.port2 = ports[1];
        }

        globalThis.MessagePort = MessagePort;
        globalThis.MessageChannel = MessageChannel;

        // Internal hook for the Worker fallback path so it can dispatch
        // to a port without re-implementing the dispatch logic.
        globalThis.__nsMessagePortDispatch = __dispatchMessage;
    })();
    "#;

    let Some(source) = v8::String::new(scope, source) else {
        return;
    };

    if let Some(script) = v8::Script::compile(scope, source, None) {
        script.run(scope);
    }
}
