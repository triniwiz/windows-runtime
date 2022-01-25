#pragma once
#include "Common.h"
namespace org {
	namespace nativescript {
		class SimpleAllocator : public v8::ArrayBuffer::Allocator {
		public:
			SimpleAllocator();

			~SimpleAllocator() override;

			void* Allocate(size_t length) override;

			void* AllocateUninitialized(size_t length) override;

			void Free(void* data, size_t length) override;
		};
	}
}