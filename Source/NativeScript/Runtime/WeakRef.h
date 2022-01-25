#pragma once
#include "Common.h"

namespace org {
	namespace nativescript {
		class WeakRef {
		public:
			static void Init(v8::Local<v8::Context> context);
		};
	}
}