#pragma once
#include "pch.h"
#include "Declaration.h"
#include <vector>

#include <comdef.h>
#include <wrl.h>
#include <wrl/wrappers/corewrappers.h>
#include <wrl/client.h>
#include <Rometadataresolution.h>
#include "IteratorRange.h"


namespace org {
    namespace nativescript {

        class NamespaceDeclaration final : public Declaration {
        public:
            typedef Declaration Base;

            using ChildrenIterator = std::vector<std::wstring>::const_iterator;

            explicit NamespaceDeclaration(const wchar_t* fullName);

            virtual std::wstring name() const override;

            virtual std::wstring fullName() const override;

            // Incomplete
            IteratorRange<ChildrenIterator> children();

        private:
            std::wstring _fullName;
            std::vector<std::wstring> _children;
        };
    }
}