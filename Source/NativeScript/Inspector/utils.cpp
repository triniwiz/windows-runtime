//
// Created by triniwiz on 19/01/2022.
//

#include "utils.h"
#include "Helpers.h"

using namespace v8;

std::string v8_inspector::GetMIMEType(std::string filePath) {
    return std::string(get_mime_type(filePath));
}

std::string v8_inspector::ToStdString(const StringView& value) {
    std::vector<uint16_t> buffer(value.length());
    for (size_t i = 0; i < value.length(); i++) {
        if (value.is8Bit()) {
            buffer[i] = value.characters8()[i];
        } else {
            buffer[i] = value.characters16()[i];
        }
    }

    std::u16string value16(buffer.begin(), buffer.end());

    std::wstring_convert<std::codecvt_utf8_utf16<char16_t>, char16_t> convert;
    std::string result = convert.to_bytes(value16);

    return result;
}

Local<v8::Function> v8_inspector::GetDebuggerFunction(Local<Context> context, std::string domain, std::string functionName, Local<Object>& domainDebugger) {
    auto it = JsV8InspectorClient::Domains.find(domain);
    if (it == JsV8InspectorClient::Domains.end()) {
        return Local<v8::Function>();
    }

    Isolate* isolate = context->GetIsolate();
    domainDebugger = it->second->Get(isolate);

    Local<Value> value;
    bool success = domainDebugger->Get(context, ToV8String(isolate, functionName)).ToLocal(&value);
    if (success && !value.IsEmpty() && value->IsFunction()) {
        return value.As<v8::Function>();
    }

    return Local<v8::Function>();
}

std::string v8_inspector::GetDomainMethod(Isolate* isolate, const Local<Object>& arg, std::string domain) {
    Local<Context> context = isolate->GetCurrentContext();
    Local<Value> value;
    arg->Get(context, ToV8String(isolate, "method")).ToLocal(&value);
    //assert(arg->Get(context, ToV8String(isolate, "method")).ToLocal(&value));
    std::string method = ToString(isolate, value);

    if (method.empty()) {
        return "";
    }

    size_t pos = method.find(domain);
    if (pos == std::string::npos) {
        return "";
    }

    return method.substr(pos + domain.length());
}