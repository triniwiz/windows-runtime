#pragma once
#include <hstring.h>
#include <Rometadataresolution.h>
#include <wrl.h>
#include "Helpers.h"

namespace org {
	namespace nativescript {
        class MetadataReader {
        public:
            static void findByName(const std::string& fullName);
            static void findByName(const wchar_t* fullName);
            static void findByName(HSTRING fullName);
        };
	}
}