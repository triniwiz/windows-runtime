//
// Created by triniwiz on 18/01/2022.
//

#ifndef WINDOWS_RUNTIME_V8_NS_DEBUGGER_AGENT_IMPL_H
#define WINDOWS_RUNTIME_V8_NS_DEBUGGER_AGENT_IMPL_H
namespace v8_inspector {

    class NSV8DebuggerAgentImpl : public V8DebuggerAgentImpl {
    public:
        NSV8DebuggerAgentImpl(V8InspectorSessionImpl *, protocol::FrontendChannel *, protocol::DictionaryValue *state);

        NSV8DebuggerAgentImpl(const NSV8DebuggerAgentImpl &) = delete;

        NSV8DebuggerAgentImpl &operator=(const NSV8DebuggerAgentImpl &) = delete;

        Response getPossibleBreakpoints(
                std::unique_ptr <protocol::Debugger::Location> start,
                Maybe <protocol::Debugger::Location> end,
                Maybe<bool> restrictToFunction,
                std::unique_ptr <protocol::Array<protocol::Debugger::BreakLocation>> *locations) override;
    };

}

#endif //WINDOWS_RUNTIME_V8_NS_DEBUGGER_AGENT_IMPL_H
