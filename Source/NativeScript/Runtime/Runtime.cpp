#include "pch.h"
#include "Runtime.h"
#include "SimpleAllocator.h"
#include "Caches.h"
#include "Helpers.h"
#include "Console.h"
#include "NativeScriptException.h"
#include "WeakRef.h"
#include "MetadataBuilder.h"

using namespace v8;
namespace org {
	namespace nativescript {
		SimpleAllocator allocator_;

		Runtime::Runtime()
		{
			currentRuntime_ = this;
		}

		Runtime::~Runtime()
		{
			this->isolate_->Dispose();


			currentRuntime_ = nullptr;
		}

		v8::Isolate* Runtime::CreateIsolate()
		{
			if (!mainThreadInitialized_) {
				Runtime::platform_ = platform::NewSingleThreadedDefaultPlatform();
				V8::InitializePlatform(Runtime::platform_.get());

				v8::V8::Initialize();
				std::string flags("--expose_gc");

				V8::SetFlagsFromString(flags.c_str(), flags.size());
			}

			Isolate::CreateParams params;
			params.array_buffer_allocator = &allocator_;

			Isolate* isolate = Isolate::New(params);

			isolates_.emplace_back(isolate);

			return isolate;
		}
		void Runtime::Init(v8::Isolate* isolate)
		{
			std::shared_ptr<Caches> cache = Caches::Get(isolate);
			Isolate::Scope isolate_scope(isolate);
			HandleScope handle_scope(isolate);


			Local<FunctionTemplate> globalFunctionTemplate = FunctionTemplate::New(isolate);
			globalFunctionTemplate->SetClassName(ToV8String(isolate, "NativeScriptGlobalObject"));
			Local<ObjectTemplate> globalTemplate = ObjectTemplate::New(isolate, globalFunctionTemplate);

			DefinePerformanceObject(isolate, globalTemplate);
			DefineTimeMethod(isolate, globalTemplate);

			MetadataBuilder::RegisterConstantsOnGlobalObject(isolate, globalTemplate, mainThreadInitialized_);
			
			isolate->SetCaptureStackTraceForUncaughtExceptions(true, 100, v8::StackTrace::kOverview);
			isolate->AddMessageListener(NativeScriptException::OnUncaughtError);


			Local<Context> context = Context::New(isolate, nullptr, globalTemplate);

			context->Enter();

			DefineGlobalObject(context);
			DefineCollectFunction(context);
			Console::Init(context);
			WeakRef::Init(context);


			this->moduleInternal_ = std::make_unique<ModuleInternal>(context);

			cache->SetContext(context);

			mainThreadInitialized_ = true;

			this->isolate_ = isolate;
			
		}
		void Runtime::RunMainScript()
		{
			Isolate* isolate = this->GetIsolate();
			v8::Locker locker(isolate);
			Isolate::Scope isolate_scope(isolate);
			HandleScope handle_scope(isolate);
			this->moduleInternal_->RunModule(isolate, ".\\");
		}
		void Runtime::RunModule(const std::string moduleName) {
			Isolate* isolate = this->GetIsolate();
			Isolate::Scope isolate_scope(isolate);
			HandleScope handle_scope(isolate);
			this->moduleInternal_->RunModule(isolate, moduleName);
		}

		v8::Isolate* Runtime::GetIsolate()
		{
			return this->isolate_;
		}
		Runtime* Runtime::GetCurrentRuntime()
		{
			return Runtime::currentRuntime_;
		}
		std::shared_ptr<v8::Platform> Runtime::GetPlatform()
		{
			return platform_;
		}
		bool Runtime::IsAlive(v8::Isolate* isolate)
		{
			return std::find(Runtime::isolates_.begin(), Runtime::isolates_.end(), isolate) != Runtime::isolates_.end();
		}
		void Runtime::DefineGlobalObject(v8::Local<v8::Context> context)
		{
			Isolate* isolate = context->GetIsolate();
			Local<Object> global = context->Global();

			const v8::PropertyAttribute readonlyFlags = static_cast<v8::PropertyAttribute>(v8::PropertyAttribute::DontDelete | v8::PropertyAttribute::ReadOnly);

			if (!global->DefineOwnProperty(context, ToV8String(isolate, "global"), global, readonlyFlags).FromMaybe(false)) {
				// TODO assert
			}

			if (mainThreadInitialized_ && !global->DefineOwnProperty(context, ToV8String(isolate, "self"), global, readonlyFlags).FromMaybe(false)) {
				// TODO assert
			}

		}
		void Runtime::DefineCollectFunction(v8::Local<v8::Context> context)
		{
			Isolate* isolate = context->GetIsolate();
			Local<Object> global = context->Global();

			Local<Value> value;

			bool success = global->Get(context, ToV8String(isolate, "gc")).ToLocal(&value);

			// TODO assert

			if (value.IsEmpty() || value->IsFunction()) {
				return;
			}
			Local<v8::Function> gcFunc = value.As<v8::Function>();
			const v8::PropertyAttribute readonlyFlags = static_cast<v8::PropertyAttribute>(v8::PropertyAttribute::DontDelete | v8::PropertyAttribute::ReadOnly);
			success = global->DefineOwnProperty(context, ToV8String(isolate, "__collect"), gcFunc, readonlyFlags).FromMaybe(false);
			// TODO assert
		}
		void Runtime::DefinePerformanceObject(v8::Isolate* isolate, v8::Local<v8::ObjectTemplate> globalTemplate)
		{
			Local<ObjectTemplate> performanceTemplate = ObjectTemplate::New(isolate);

			Local<FunctionTemplate> nowFunctionTemplate = FunctionTemplate::New(isolate, PerformanceNowCallback);
			performanceTemplate->Set(ToV8String(isolate, "now"), nowFunctionTemplate);


			globalTemplate->Set(
				ToV8String(isolate, "performance"), performanceTemplate
			);

		}
		void Runtime::PerformanceNowCallback(const v8::FunctionCallbackInfo<v8::Value>& args)
		{
			std::chrono::system_clock::time_point now = std::chrono::system_clock::now();
			std::chrono::milliseconds timestampMs = std::chrono::duration_cast<std::chrono::milliseconds>(now.time_since_epoch());
			double result = timestampMs.count();
			args.GetReturnValue().Set(result);
		}
		void Runtime::DefineTimeMethod(v8::Isolate* isolate, v8::Local<v8::ObjectTemplate> globalTemplate)
		{
			Local<FunctionTemplate> timeFunctionTemplate = FunctionTemplate::New(isolate, [](const v8::FunctionCallbackInfo<v8::Value>& args) {
				auto nano = std::chrono::time_point_cast<std::chrono::nanoseconds>(std::chrono::system_clock::now());
				double duration = nano.time_since_epoch().count() / 1000000.0;
				args.GetReturnValue().Set(duration);
				});
			globalTemplate->Set(ToV8String(isolate, "__time"), timeFunctionTemplate);
		}


		std::shared_ptr<Platform> Runtime::platform_;
		std::vector<Isolate*> Runtime::isolates_;
		bool Runtime::mainThreadInitialized_ = false;
		thread_local Runtime* Runtime::currentRuntime_ = nullptr;
	}
}
