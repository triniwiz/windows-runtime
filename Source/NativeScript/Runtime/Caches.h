#pragma once
#include "Common.h"
#include "robin_hood.h"
#include "ConcurrentMap.h"

namespace org {
	namespace nativescript {
		class Caches {
		public:
			Caches(v8::Isolate* isolate);
			~Caches();
			static std::shared_ptr<Caches> Get(v8::Isolate* isolate);
			static void Remove(v8::Isolate* isolate);
			void SetContext(v8::Local < v8::Context> context);
			v8::Local<v8::Context> GetContext();
			std::unique_ptr <v8::Persistent<v8::Function>> SmartJSONStringifyFunc = std::unique_ptr<v8::Persistent<v8::Function>>(
				nullptr);

			robin_hood::unordered_map<std::string, double> Timers;

		private:
			static std::shared_ptr<ConcurrentMap<v8::Isolate*, std::shared_ptr<Caches>>> perIsolateCache_;
			std::shared_ptr<v8::Persistent<v8::Context>> context_;
			v8::Isolate* isolate_;


		};
	}
}