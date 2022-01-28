#include "pch.h"
#include "MetadataReader.h"

const wchar_t* const WINDOWS_W{ L"Windows" };
const wchar_t* const SYSTEM_ENUM_W{ L"System.Enum" };
const wchar_t* const SYSTEM_VALUETYPE_W{ L"System.ValueType" };
const wchar_t* const SYSTEM_MULTICASTDELEGATE_W{ L"System.MulticastDelegate" };

using namespace Microsoft::WRL::Wrappers;
using namespace Microsoft::WRL;

#include <array>

namespace {
    const size_t MAX_IDENTIFIER_LENGTH{ 511 };
    using identifier = std::array<wchar_t, MAX_IDENTIFIER_LENGTH + 1>;
}

namespace org {
	namespace nativescript {


        
        std::shared_ptr<Declaration> MetadataReader::findByName(const std::string& fullName) {
            std::wstring name(fullName.begin(), fullName.end());
            findByName(name.c_str());
        }

        std::shared_ptr<Declaration> MetadataReader::findByName(const wchar_t* fullName) {
			HStringReference fullNameRef{ fullName };
			findByName(fullNameRef.Get());
		}


        std::shared_ptr<Declaration> MetadataReader::findByName(HSTRING fullName) {
            if (WindowsGetStringLen(fullName) == 0) {
                return nullptr; //make_shared<NamespaceDeclaration>(L"");
            }

            ComPtr<IMetaDataImport2> metadata;
            mdTypeDef token{ mdTokenNil };

            HRESULT getMetadataFileResult{ RoGetMetaDataFile(fullName, nullptr, nullptr, metadata.GetAddressOf(), &token) };

            if (FAILED(getMetadataFileResult)) {
                if (getMetadataFileResult == RO_E_METADATA_NAME_IS_NAMESPACE) {
                    std::wstring ns(WindowsGetStringRawBuffer(fullName, nullptr));
                    Log("NamespaceDeclaration: " + std::string(ns.begin(), ns.end()));
                   return std::make_shared<NamespaceDeclaration>(WindowsGetStringRawBuffer(fullName, nullptr));
                }

                return nullptr;
            }

            DWORD flags{ 0 };
            mdToken parentToken{ mdTokenNil };
            Assert(SUCCEEDED(metadata->GetTypeDefProps(token, nullptr, 0, nullptr, &flags, &parentToken)));

            if (IsTdClass(flags)) {
                identifier parentName;

                switch (TypeFromToken(parentToken)) {
                case mdtTypeDef: {
                    Assert(SUCCEEDED(metadata->GetTypeDefProps(parentToken, parentName.data(), parentName.size(), nullptr, nullptr, nullptr)));
                    break;
                }

                case mdtTypeRef: {
                    Assert(SUCCEEDED(metadata->GetTypeRefProps(parentToken, nullptr, parentName.data(), parentName.size(), nullptr)));
                    break;
                }

                default:
                    Assert(false);
                }

                if (wcscmp(parentName.data(), SYSTEM_ENUM_W) == 0) {
                   // return make_shared<EnumDeclaration>(metadata.Get(), token);

                    Log("EnumDeclaration: ");

                }

                if (wcscmp(parentName.data(), SYSTEM_VALUETYPE_W) == 0) {
                   // return make_shared<StructDeclaration>(metadata.Get(), token);
                    Log("StructDeclaration: ");
                }

                if (wcscmp(parentName.data(), SYSTEM_MULTICASTDELEGATE_W) == 0) {
                    if (wcsstr(WindowsGetStringRawBuffer(fullName, nullptr), L"`")) {
                        //return make_shared<GenericDelegateDeclaration>(metadata.Get(), token);
                        Log("GenericDelegateDeclaration: ");
                    }
                    else {
                        // return make_shared<DelegateDeclaration>(metadata.Get(), token);
                        Log("DelegateDeclaration: ");
                    }
                }

               // return  make_shared<ClassDeclaration>(metadata.Get(), token);

                Log("ClassDeclaration: ");
            }

            if (IsTdInterface(flags)) {
                if (wcsstr(WindowsGetStringRawBuffer(fullName, nullptr), L"`")) {
                    //return make_shared<GenericInterfaceDeclaration>(metadata.Get(), token);

                    Log("GenericInterfaceDeclaration: ");
                }
                else {
                   // return make_shared<InterfaceDeclaration>(metadata.Get(), token);

                    Log("InterfaceDeclaration: ");
                }
            }

            //ASSERT_NOT_REACHED();
            Assert(false);
        }

	}
}