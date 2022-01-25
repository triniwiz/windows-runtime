//
// Created by triniwiz on 18/01/2022.
//

#include "v8-overlay-agent-impl.h"

namespace v8_inspector {

    namespace OverlayAgentState {
        static const char overlayEnabled[] = "overlayEnabled";
    }

    V8OverlayAgentImpl::V8OverlayAgentImpl(V8InspectorSessionImpl *session, protocol::FrontendChannel *frontendChannel,
                                           protocol::DictionaryValue *state)
            : m_frontend(frontendChannel),
              m_state(state),
              m_enabled(false) {
    }

    V8OverlayAgentImpl::~V8OverlayAgentImpl() {}

    DispatchResponse V8OverlayAgentImpl::enable() {
        if (m_enabled) {
            return DispatchResponse::ServerError("Overlay Agent already enabled!");
        }

        m_state->setBoolean(OverlayAgentState::overlayEnabled, true);
        m_enabled = true;

        return DispatchResponse::Success();
    }

    DispatchResponse V8OverlayAgentImpl::disable() {
        if (!m_enabled) {
            return DispatchResponse::Success();
        }

        m_state->setBoolean(OverlayAgentState::overlayEnabled, false);

        m_enabled = false;

        return DispatchResponse::Success();
    }

    DispatchResponse V8OverlayAgentImpl::setShowFPSCounter(bool in_show) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setPausedInDebuggerMessage(const Maybe <String> in_message) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setShowAdHighlights(bool in_show) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse
    V8OverlayAgentImpl::highlightNode(std::unique_ptr <protocol::Overlay::HighlightConfig> in_highlightConfig,
                                      Maybe<int> in_nodeId,
                                      Maybe<int> in_backendNodeId,
                                      Maybe <String> in_objectId,
                                      Maybe <String> in_selector) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::highlightFrame(const String &in_frameId,
                                                        Maybe <protocol::DOM::RGBA> in_contentColor,
                                                        Maybe <protocol::DOM::RGBA> in_contentOutlineColor) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::hideHighlight() {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::getHighlightObjectForTest(int in_nodeId,
                                                                   Maybe<bool> in_includeDistance,
                                                                   Maybe<bool> in_includeStyle,
                                                                   std::unique_ptr <protocol::DictionaryValue> *out_highlight) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::highlightQuad(std::unique_ptr <protocol::Array<double>> in_quad,
                                                       Maybe <protocol::DOM::RGBA> in_color,
                                                       Maybe <protocol::DOM::RGBA> in_outlineColor) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::highlightRect(int in_x, int in_y, int in_width, int in_height,
                                                       Maybe <protocol::DOM::RGBA> in_color,
                                                       Maybe <protocol::DOM::RGBA> in_outlineColor) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setInspectMode(const String &in_mode,
                                                        Maybe <protocol::Overlay::HighlightConfig> in_highlightConfig) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setShowDebugBorders(bool in_show) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setShowPaintRects(bool in_result) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setShowScrollBottleneckRects(bool in_show) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setShowHitTestBorders(bool in_show) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setShowViewportSizeOnResize(bool in_show) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

    DispatchResponse V8OverlayAgentImpl::setShowLayoutShiftRegions(bool in_result) {
        return protocol::DispatchResponse::ServerError("Protocol command not supported.");
    }

}