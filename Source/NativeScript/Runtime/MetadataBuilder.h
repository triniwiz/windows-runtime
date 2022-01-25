#pragma once
#include "Common.h"

namespace org {
	namespace nativescript {
		class MetadataBuilder {
		public:
			static void RegisterConstantsOnGlobalObject(v8::Isolate* isolate, v8::Local<v8::ObjectTemplate> globalTemplate, bool isWorkerThread);

		private:

			static void GlobalPropertyGetter(v8::Local<v8::Name> property, const v8::PropertyCallbackInfo<v8::Value>& info);

			struct GlobalHandlerContext {
				GlobalHandlerContext(bool isWorkerThread) : isWorkerThread_(isWorkerThread) {
				}
				bool isWorkerThread_;
			};
		};
	}
}