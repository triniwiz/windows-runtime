//
// Created by triniwiz on 19/01/2022.
//

#ifndef WINDOWS_RUNTIME_UTILS_H
#define WINDOWS_RUNTIME_UTILS_H

#include <codecvt>
#include <locale>

#include "include/v8-inspector.h"

namespace v8_inspector {

    std::string GetMIMEType(std::string filePath);

    std::string ToStdString(const v8_inspector::StringView &value);

    v8::Local <v8::Function>
    GetDebuggerFunction(v8::Local <v8::Context> context, std::string domain, std::string functionName,
                        v8::Local <v8::Object> &domainDebugger);

    std::string GetDomainMethod(v8::Isolate *isolate, const v8::Local <v8::Object> &arg, std::string domain);

    class NetworkRequestData {
    public:
        NetworkRequestData(std::u16string data, bool hasTextContent) : data_(data), hasTextContent_(hasTextContent) {
        }

        const char16_t *GetData() {
            return this->data_.data();
        }

        const bool HasTextContent() {
            return this->hasTextContent_;
        }

    private:
        std::u16string data_;
        bool hasTextContent_;
    };

}

#endif //WINDOWS_RUNTIME_UTILS_H
