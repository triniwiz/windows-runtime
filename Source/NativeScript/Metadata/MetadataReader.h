#pragma once
#include <hstring.h>
#include <Rometadataresolution.h>
#include <wrl.h>
#include "Helpers.h"
#include "NamespaceDeclaration.h"

namespace org {
	namespace nativescript {
        class MetadataReader {
        public:
            static std::shared_ptr<Declaration> findByName(const std::string& fullName);
            static std::shared_ptr<Declaration> findByName(const wchar_t* fullName);
            static std::shared_ptr<Declaration> findByName(HSTRING fullName);
        };
	}
}