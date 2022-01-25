#include "pch.h"
#include "Helpers.h"
#include "Caches.h"
#include "Runtime.h"
#include "RuntimeConfig.h"
#include <sstream>
#include <iostream>
#include <consoleapi.h>
#include <PathCch.h>
#include <fileapi.h>
#include <sys/stat.h>
#include <stdio.h>

namespace {
    const int BUFFER_SIZE = 1024 * 1024;
    char* Buffer = new char[BUFFER_SIZE];
    uint8_t* BinBuffer = new uint8_t[BUFFER_SIZE];
}

using namespace v8;
namespace org {
	namespace nativescript {

        v8::Local <v8::String> ToV8String(v8::Isolate* isolate, std::string value) {
            return v8::String::NewFromUtf8(isolate,
                value.c_str(),
                v8::NewStringType::kNormal,
                (int)value.length()).ToLocalChecked();
        }

        std::string ToString(v8::Isolate* isolate, v8::Local <v8::String> value) {
            if (value.IsEmpty()) {
                return std::string();
            }

            if (value->IsStringObject()) {
                v8::Local <v8::String> string = value.As<v8::StringObject>()->ValueOf();
                return ToString(isolate, string);
            }

            v8::String::Utf8Value result(isolate, value);

            const char* string = *result;

            if (string == nullptr) {
                return std::string();
            }
            return std::string(string);

        }

        v8::Local <v8::Function> GetSmartJSONStringifyFunction(v8::Isolate* isolate) {
            std::shared_ptr <Caches> caches = Caches::Get(isolate);
            if (caches->SmartJSONStringifyFunc != nullptr) {
                return caches->SmartJSONStringifyFunc->Get(isolate);
            }

            std::string smartStringifyFunctionScript =
                "(function () {\n"
                "    function smartStringify(object) {\n"
                "        const seen = [];\n"
                "        var replacer = function (key, value) {\n"
                "            if (value != null && typeof value == \"object\") {\n"
                "                if (seen.indexOf(value) >= 0) {\n"
                "                    if (key) {\n"
                "                        return \"[Circular]\";\n"
                "                    }\n"
                "                    return;\n"
                "                }\n"
                "                seen.push(value);\n"
                "            }\n"
                "            return value;\n"
                "        };\n"
                "        return JSON.stringify(object, replacer, 2);\n"
                "    }\n"
                "    return smartStringify;\n"
                "})();";

            v8::Local <v8::String> source = ToV8String(isolate, smartStringifyFunctionScript);
            v8::Local <v8::Context> context = isolate->GetCurrentContext();

            v8::Local <v8::Script> script;
            bool success = v8::Script::Compile(context, source).ToLocal(&script);

            // TODO assert

            if (script.IsEmpty()) {
                return v8::Local<v8::Function>();
            }

            v8::Local <v8::Value> result;
            success = script->Run(context).ToLocal(&result);
            // TODO assert

            if (result.IsEmpty() && !result->IsFunction()) {
                return v8::Local<v8::Function>();
            }

            v8::Local <v8::Function> smartStringifyFunction = result.As<v8::Function>();

            caches->SmartJSONStringifyFunc =
                std::make_unique < v8::Persistent < v8::Function >>(isolate, smartStringifyFunction);

            return smartStringifyFunction;
        }

        std::string ReplaceAll(const std::string source, std::string find, std::string replacement) {
            std::string result = source;
            size_t pos = result.find(find);
            while (pos != std::string::npos) {
                result.replace(pos, find.size(), replacement);
                pos = result.find(find, pos + replacement.size());
            }

            return result;
        }


        const std::string GetStackTrace(v8::Isolate* isolate) {
            v8::Local <v8::StackTrace> stack = v8::StackTrace::CurrentStackTrace(isolate, 10,
                v8::StackTrace::StackTraceOptions::kDetailed);
            int framesCount = stack->GetFrameCount();
            std::stringstream ss;
            for (int i = 0; i < framesCount; i++) {
                v8::Local <v8::StackFrame> frame = stack->GetFrame(isolate, i);
                ss << BuildStacktraceFrameMessage(isolate, frame) << std::endl;
            }
            return ss.str();
        }

        const std::string BuildStacktraceFrameMessage(v8::Isolate* isolate, v8::Local <v8::StackFrame> frame) {
            std::stringstream ss;

            v8::Local <v8::String> functionName = frame->GetFunctionName();
            std::string functionNameStr = ToString(isolate, functionName);
            if (functionNameStr.empty()) {
                functionNameStr = "<anonymous>";
            }

            if (frame->IsConstructor()) {
                ss << "at new " << functionNameStr << " (" << BuildStacktraceFrameLocationPart(isolate, frame) << ")";
            }
            else if (frame->IsEval()) {
                ss << "eval at " << BuildStacktraceFrameLocationPart(isolate, frame) << std::endl;
            }
            else {
                ss << "at " << functionNameStr << " (" << BuildStacktraceFrameLocationPart(isolate, frame) << ")";
            }

            std::string stringResult = ss.str();

            return stringResult;
        }


        const std::string BuildStacktraceFrameLocationPart(v8::Isolate* isolate, v8::Local <v8::StackFrame> frame) {
            std::stringstream ss;

            v8::Local <v8::String> scriptName = frame->GetScriptNameOrSourceURL();
            std::string scriptNameStr = ToString(isolate, scriptName);
            scriptNameStr = std::string(ReplaceAll(scriptNameStr, RuntimeConfig.BasePath, ""));

            if (scriptNameStr.length() < 1) {
                ss << "VM";
            }
            else {
                ss << scriptNameStr << ":" << frame->GetLineNumber() << ":" << frame->GetColumn();
            }

            std::string stringResult = ss.str();

            return stringResult;
        }


        const std::string GetCurrentScriptUrl(v8::Isolate* isolate) {
            v8::Local <v8::StackTrace> stack = v8::StackTrace::CurrentStackTrace(isolate, 1,
                v8::StackTrace::StackTraceOptions::kDetailed);
            int framesCount = stack->GetFrameCount();
            if (framesCount > 0) {
                v8::Local <v8::StackFrame> frame = stack->GetFrame(isolate, 0);
                return BuildStacktraceFrameLocationPart(isolate, frame);
            }

            return "";
        }


        v8::Local <v8::String>
            JsonStringifyObject(v8::Local <v8::Context> context, v8::Local <v8::Value> value, bool handleCircularReferences) {
            v8::Isolate* isolate = context->GetIsolate();
            if (value.IsEmpty()) {
                return v8::String::Empty(isolate);
            }

            if (handleCircularReferences) {
                v8::Local <v8::Function> smartJSONStringifyFunction = GetSmartJSONStringifyFunction(isolate);

                if (!smartJSONStringifyFunction.IsEmpty()) {
                    if (value->IsObject()) {
                        v8::Local <v8::Value> resultValue;
                        v8::TryCatch tc(isolate);

                        v8::Local <v8::Value> args[] = {
                                value->ToObject(context).ToLocalChecked()
                        };
                        bool success = smartJSONStringifyFunction->Call(context, v8::Undefined(isolate), 1, args).ToLocal(
                            &resultValue);

                        if (success && !tc.HasCaught()) {
                            return resultValue->ToString(context).ToLocalChecked();
                        }
                    }
                }
            }

            v8::Local <v8::String> resultString;
            v8::TryCatch tc(isolate);
            bool success = v8::JSON::Stringify(context, value->ToObject(context).ToLocalChecked()).ToLocal(&resultString);

            if (!success && tc.HasCaught()) {
                LogError(isolate, tc);
                return v8::Local<v8::String>();
            }

            return resultString;
        }


        void LogError(v8::Isolate* isolate, v8::TryCatch& tc) {
            if (!tc.HasCaught()) {
                return;
            }

            Log("Native stack trace:");
            LogBacktrace();

            v8::Local <v8::Value> stack;
            v8::Local <v8::Context> context = isolate->GetCurrentContext();
            bool success = tc.StackTrace(context).ToLocal(&stack);
            if (!success || stack.IsEmpty()) {
                return;
            }

            v8::Local <v8::String> stackV8Str;
            success = stack->ToDetailString(context).ToLocal(&stackV8Str);
            if (!success || stackV8Str.IsEmpty()) {
                return;
            }

            std::string stackTraceStr = ToString(isolate, stackV8Str);

            stackTraceStr = std::string(ReplaceAll(stackTraceStr, RuntimeConfig.BasePath, ""));

            Log("JavaScript error:");
            Log(stackTraceStr.c_str());
        }


        void LogBacktrace(int skip) {
            //    void *callstack[128];
            //    const int nMaxFrames = sizeof(callstack) / sizeof(callstack[0]);
            //    char buf[1024];
            //    int nFrames = backtrace(callstack, nMaxFrames);
            //    char **symbols = backtrace_symbols(callstack, nFrames);
            //
            //    for (int i = skip; i < nFrames; i++) {
            //        Dl_info info;
            //        if (dladdr(callstack[i], &info) && info.dli_sname) {
            //            char *demangled = NULL;
            //            int status = -1;
            //            if (info.dli_sname[0] == '_') {
            //                demangled = abi::__cxa_demangle(info.dli_sname, NULL, 0, &status);
            //            }
            //            snprintf(buf,
            //                     sizeof(buf),
            //                     "%-3d %*p %s + %zd\n",
            //                     i,
            //                     int(2 + sizeof(void *) * 2),
            //                     callstack[i],
            //                     status == 0 ? demangled : info.dli_sname == 0 ? symbols[i] : info.dli_sname,
            //                     (char *) callstack[i] - (char *) info.dli_saddr);
            //            free(demangled);
            //        } else {
            //            snprintf(buf, sizeof(buf), "%-3d %*p %s\n", i, int(2 + sizeof(void *) * 2), callstack[i], symbols[i]);
            //        }
            //        Log(buf);
            //    }
            //    free(symbols);
            //    if (nFrames == nMaxFrames) {
            //        Log("[truncated]");
            //    }
        }

        void Log(const char* text) {
            Log(std::string(text));
        }

        void Log(std::string text) {
            std::string out = text.append("\n");
            OutputDebugStringA(out.c_str());
            /*
            HANDLE handle = GetStdHandle(STD_OUTPUT_HANDLE);
            DWORD lpMode;
            bool is_console = GetConsoleMode(handle, &lpMode);


            DWORD written;
            std::string out = text.append("\n");


            if (is_console) {
                WriteConsoleA(handle, out.c_str(), (DWORD)strlen(out.c_str()), &written, nullptr);
            }
            else {
                WriteFile(handle, out.c_str(), (DWORD)strlen(out.c_str()), &written, nullptr);
            }
            */

        }
        ;
        void Assert(bool condition, v8::Isolate* isolate) {
            if (!RuntimeConfig.IsDebug) {
                assert(condition);
                return;
            }

            if (condition) {
                return;
            }

            if (isolate == nullptr) {
                Runtime* runtime = Runtime::GetCurrentRuntime();
                if (runtime != nullptr) {
                    isolate = runtime->GetIsolate();
                }
            }

            if (isolate == nullptr) {
                Log("====== Assertion failed ======");
                Log("Native stack trace:");
                LogBacktrace();
                assert(false);
                return;
            }

            Log("====== Assertion failed ======");
            Log("Native stack trace:");
            LogBacktrace();

            Log("JavaScript stack trace:");
            std::string stack = GetStackTrace(isolate);
            Log(stack.c_str());
            assert(false);
        }
        std::string JoinPath(const std::string& Base, const std::string& Path)
        {
            WCHAR buffer[MAX_PATH] = L"";

            std::wstring base(Base.begin(), Base.end());
            std::wstring path(Path.begin(), Path.end());

            if (!FAILED(PathCchCombine(buffer, MAX_PATH, base.c_str(), path.c_str()))) {
                std::wstring result(buffer);
                return std::string(result.begin(), result.end());
            }

            return std::string();
        }
        std::string JoinPath(const char* Base, const char* Path)
        {
            return JoinPath(std::string(Base), std::string(Path));
        }
        bool Exists(const char* fullPath)
        {
            struct stat file_info;
            return stat(fullPath, &file_info) == 0;
        }
        std::string GetFileName(std::string const& path)
        {
             return path.substr(path.find_last_of("/\\") + 1);
        }
        std::string GetFileExtension(std::string const& path)
        {
            return path.substr(path.find_last_of('.') + 1);
        }
        bool IsStringEqual(std::string const& a, std::string const& b)
        {
            return strcmp(a.c_str(), b.c_str()) == 0;
        }
        std::string GetPathByDeletingLastComponent(std::string const& path)
        {
            return path.substr(0, path.find_last_of("/\\"));
        }
        bool GetIsDirectory(std::string const& path)
        {
            struct stat file_info;
            if (stat(path.c_str(), &file_info) == 0) {
                if (file_info.st_mode & S_IFDIR) {
                    return true;
                }
            }

            return false;
        }
        LPBYTE ReadFileInternal(std::string const& path)
        {
            HANDLE hFile = INVALID_HANDLE_VALUE;

            BOOL fSuccess = FALSE;
            DWORD dwRetVal = 0;
            UINT uRetVal = 0;

            DWORD dwBytesRead = 0;
            DWORD dwBytesWritten = 0;

            std::wstring file(path.begin(), path.end());
            hFile = CreateFile2(file.c_str(), GENERIC_READ, 0, OPEN_EXISTING, nullptr);

            if (!hFile) {
                return nullptr;
            }

            FILE_ALLOCATION_INFO allocationInfo;
            bool result = GetFileInformationByHandleEx(hFile, FileAllocationInfo, &allocationInfo, sizeof(FILE_ALLOCATION_INFO));

            if (!result) {
                return nullptr;
            }

            DWORD bytes;
            size_t bytesToRead = allocationInfo.AllocationSize.QuadPart;
            // TODO auto free ??
            LPBYTE fileBuffer = (LPBYTE)malloc(bytesToRead);

            if (!ReadFile(hFile, fileBuffer, bytesToRead, &bytes, 0)) {
                CloseHandle(hFile);
                return nullptr;
            }
            else {
                CloseHandle(hFile);
            }

           
            return fileBuffer;
        }

        v8::Local<v8::String> ReadModule(v8::Isolate* isolate, const std::string& filePath) {
            std::string content = ReadText(filePath);
            
            std::string result(Helpers::MODULE_PROLOGUE);

            result.reserve(content.length() + 1024);
            result += content;
            result += Helpers::MODULE_EPILOGUE;

            return ToV8String(isolate, result);
        }

        const char* ReadText(const std::string& filePath, long& length, bool& isNew) {
            FILE* file = fopen(filePath.c_str(), "rb");
            if (file == nullptr) {
                Assert(false);
            }

            fseek(file, 0, SEEK_END);

            length = ftell(file);
            isNew = length > BUFFER_SIZE;

            rewind(file);

            if (isNew) {
                char* newBuffer = new char[length];
                fread(newBuffer, 1, length, file);
                fclose(file);

                return newBuffer;
            }

            fread(Buffer, 1, length, file);
            fclose(file);

            return Buffer;
        }

        std::string ReadText(const std::string& file) {
            long length;
            bool isNew;
            const char* content = ReadText(file, length, isNew);

            std::string result(content, length);

            if (isNew) {
                delete[] content;
            }

            return result;
        }

        uint8_t* ReadBinary(const std::string path, long& length, bool& isNew) {
            length = 0;
            std::ifstream ifs(path);
            if (ifs.fail()) {
                return nullptr;
            }

            FILE* file = fopen(path.c_str(), "rb");
            if (!file) {
                return nullptr;
            }

            fseek(file, 0, SEEK_END);
            length = ftell(file);
            rewind(file);

            isNew = length > BUFFER_SIZE;

            if (isNew) {
                uint8_t* data = new uint8_t[length];
                fread(data, sizeof(uint8_t), length, file);
                fclose(file);
                return data;
            }

            fread(BinBuffer, 1, length, file);
            fclose(file);

            return BinBuffer;
        }

        bool WriteBinary(const std::string& path, const void* data, long length) {
            FILE* file = fopen(path.c_str(), "wb");
            if (!file) {
                return false;
            }

            size_t writtenBytes = fwrite(data, sizeof(uint8_t), length, file);
            fclose(file);

            return writtenBytes == length;
        }

        void SetPrivateValue(const v8::Local<v8::Object>& obj, const v8::Local<v8::String>& propName, const v8::Local<v8::Value>& value) {
            Local<Context> context;
            bool success = obj->GetCreationContext().ToLocal(&context);
            Assert(success);
            Isolate* isolate = context->GetIsolate();
            Local<Private> privateKey = Private::ForApi(isolate, propName);

            if (!obj->SetPrivate(context, privateKey, value).To(&success) || !success) {
                Assert(false, isolate);
            }
        }

        Local<Value> GetPrivateValue(const Local<Object>& obj, const Local<v8::String>& propName) {
            Local<Context> context;
            bool success = obj->GetCreationContext().ToLocal(&context);
            Assert(success);
            Isolate* isolate = context->GetIsolate();
            Local<Private> privateKey = Private::ForApi(isolate, propName);

            Maybe<bool> hasPrivate = obj->HasPrivate(context, privateKey);

            Assert(!hasPrivate.IsNothing(), isolate);

            if (!hasPrivate.FromMaybe(false)) {
                return Local<Value>();
            }

            v8::Locker locker(isolate);
            Local<Value> result;
            if (!obj->GetPrivate(context, privateKey).ToLocal(&result)) {
                Assert(false, isolate);
            }

            return result;
        }

	}
}

const char* org::nativescript::Helpers::MODULE_PROLOGUE = "(function(module, exports, require, __filename, __dirname){ ";
const char* org::nativescript::Helpers::MODULE_EPILOGUE = "\n})";
int org::nativescript::Helpers::MODULE_PROLOGUE_LENGTH = std::string(org::nativescript::Helpers::MODULE_PROLOGUE).length();
