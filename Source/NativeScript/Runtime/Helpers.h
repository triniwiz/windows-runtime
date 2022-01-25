#pragma once
#include "Common.h"

namespace org {
	namespace nativescript {
		v8::Local <v8::String> ToV8String(v8::Isolate* isolate, std::string value);

		std::string ToString(v8::Isolate* isolate, v8::Local <v8::String> value);

		v8::Local <v8::Function> GetSmartJSONStringifyFunction(v8::Isolate* isolate);

		v8::Local <v8::String>
			JsonStringifyObject(v8::Local <v8::Context> context, v8::Local <v8::Value> value, bool handleCircularReferences = true);


		std::string ReplaceAll(const std::string source, std::string find, std::string replacement);

		const std::string GetStackTrace(v8::Isolate* isolate);

		const std::string BuildStacktraceFrameMessage(v8::Isolate* isolate, v8::Local <v8::StackFrame> frame);

		const std::string BuildStacktraceFrameLocationPart(v8::Isolate* isolate, v8::Local <v8::StackFrame> frame);

		const std::string GetCurrentScriptUrl(v8::Isolate* isolate);

		void LogError(v8::Isolate* isolate, v8::TryCatch& tc);

		void LogBacktrace(int skip = 1);

		void Log(std::string text);

		void Log(const char* text);

		void Assert(bool condition, v8::Isolate* isolate = nullptr);

		std::string JoinPath(const std::string& Base, const std::string& Path);

		std::string JoinPath(const char * Base, const char * Path);

		bool Exists(const char* fullPath);

		std::string GetFileName(std::string const& path);

		std::string GetFileExtension(std::string const& path);

		bool IsStringEqual(std::string const& a, std::string const& b);

		std::string GetPathByDeletingLastComponent(std::string const& path);

		bool GetIsDirectory(std::string const& path);

		LPBYTE ReadFileInternal(std::string const& path);

		v8::Local<v8::String> ReadModule(v8::Isolate* isolate, const std::string& filePath);
		const char* ReadText(const std::string& filePath, long& length, bool& isNew);
		std::string ReadText(const std::string& file);
		uint8_t* ReadBinary(const std::string path, long& length, bool& isNew);
		bool WriteBinary(const std::string& path, const void* data, long length);

		void SetPrivateValue(const v8::Local<v8::Object>& obj, const v8::Local<v8::String>& propName, const v8::Local<v8::Value>& value);
		v8::Local<v8::Value> GetPrivateValue(const v8::Local<v8::Object>& obj, const v8::Local<v8::String>& propName);

		class Helpers {
		public:
			static int MODULE_PROLOGUE_LENGTH;

			static const char* MODULE_PROLOGUE;
			static const char* MODULE_EPILOGUE;
		};
		
		
	}
}
