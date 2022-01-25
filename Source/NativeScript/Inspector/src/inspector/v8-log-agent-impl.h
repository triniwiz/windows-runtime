//
// Created by triniwiz on 18/01/2022.
//

#ifndef WINDOWS_RUNTIME_V8_LOG_AGENT_IMPL_H
#define WINDOWS_RUNTIME_V8_LOG_AGENT_IMPL_H

#include "protocol/Log.h"

namespace v8_inspector {

    class V8InspectorSessionImpl;

    using v8_inspector::protocol::DispatchResponse;

    class V8LogAgentImpl : public protocol::Log::Backend {
    public:
        V8LogAgentImpl(V8InspectorSessionImpl *session, protocol::FrontendChannel *frontend,
                       protocol::DictionaryValue *state);

        ~V8LogAgentImpl() override;

        DispatchResponse enable() override;

        DispatchResponse disable() override;

        DispatchResponse clear() override;

        DispatchResponse
        startViolationsReport(std::unique_ptr <protocol::Array<protocol::Log::ViolationSetting>> in_config) override;

        DispatchResponse stopViolationsReport() override;

        static void EntryAdded(const std::string &text, std::string verbosityLevel, std::string url, int lineNumber);

    private:
        protocol::Log::Frontend m_frontend;
        protocol::DictionaryValue *m_state;
        bool m_enabled;

        static V8LogAgentImpl *instance_;
    };

}
#endif //WINDOWS_RUNTIME_V8_LOG_AGENT_IMPL_H
