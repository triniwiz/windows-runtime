#include "pch.h"
#include "Caches.h"

using namespace v8;
namespace org {
	namespace nativescript {
		Caches::Caches(v8::Isolate* isolate): isolate_(isolate)
		{
		}
		Caches::~Caches()
		{
		}
		std::shared_ptr<Caches> Caches::Get(v8::Isolate* isolate)
		{
			std::shared_ptr<Caches> cache = Caches::perIsolateCache_->Get(isolate);
			if (cache == nullptr) {
				cache = std::make_shared<Caches>(isolate);
				Caches::perIsolateCache_->Insert(isolate,cache);
			}
			return cache;
		}

		void Caches::Remove(v8::Isolate* isolate)
		{
			Caches::perIsolateCache_->Remove(isolate);
		}

		void Caches::SetContext(v8::Local<v8::Context> context)
		{
			this->context_ = std::make_shared<Persistent<Context>>(this->isolate_, context);
		}

		v8::Local<v8::Context> Caches::GetContext()
		{
			return this->context_->Get(this->isolate_);
		}

		std::shared_ptr<ConcurrentMap<Isolate*, std::shared_ptr<Caches>>> Caches::perIsolateCache_ = std::make_shared<ConcurrentMap<Isolate*, std::shared_ptr<Caches>>>();
	}
}

