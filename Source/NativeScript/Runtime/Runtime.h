#pragma once
#include "Common.h"
#include <chrono>
#include "ModuleInternal.h"

namespace org {
	namespace nativescript {
		class Runtime {
		public:
			Runtime();
			~Runtime();
			v8::Isolate* CreateIsolate();
			void Init(v8::Isolate* isolate);
			void RunMainScript();
			void RunModule(const std::string moduleName);
			v8::Isolate* GetIsolate();
			static Runtime* GetCurrentRuntime();

			std::shared_ptr<v8::Platform> GetPlatform();

			static bool IsAlive(v8::Isolate* isolate);


		private:
			static std::shared_ptr<v8::Platform> platform_;
			static thread_local Runtime* currentRuntime_;

			static std::vector<v8::Isolate*> isolates_;
			static bool mainThreadInitialized_;

			v8::Isolate* isolate_;

			void DefineNativeScriptVersion(v8::Isolate* isolate, v8::Local <v8::ObjectTemplate> globalTemplate);

			void DefineGlobalObject(v8::Local<v8::Context> context);

			void DefineCollectFunction(v8::Local<v8::Context> context);

			void DefinePerformanceObject(v8::Isolate* isolate, v8::Local<v8::ObjectTemplate> globalTemplate);

			void DefineTimeMethod(v8::Isolate* isolate, v8::Local<v8::ObjectTemplate> globalTemplate);

			static void PerformanceNowCallback(const v8::FunctionCallbackInfo<v8::Value>& args);

			std::unique_ptr<ModuleInternal> moduleInternal_;

		};

	}
}