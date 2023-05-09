//
// Created by fortu on 28/04/2023.
//

#include "bindings.h"
#include "rust/cxx.h"

using namespace std;

int32_t BindingsCCorSigUncompressCallingConv(uint8_t* pData) {
    auto sig = static_cast<PCCOR_SIGNATURE>(pData);
    return (int32_t)CorSigUncompressCallingConv(sig);
};

int32_t BindingsCCorSigUncompressData(uint8_t* pData) {
    auto sig = static_cast<PCCOR_SIGNATURE>(pData);
    return (int32_t)CorSigUncompressData(sig);
};


int32_t BindingsCCorSigUncompressDataWithOutput(uint8_t* pData, uint32_t* output) {
    auto sig = static_cast<PCCOR_SIGNATURE>(pData);
    return (int32_t)CorSigUncompressData(sig, (ULONG*)output);
};


int32_t BindingsCCorSigUncompressElementType(uint8_t* pData) {
    auto sig = static_cast<PCCOR_SIGNATURE>(pData);
    return (int32_t)CorSigUncompressElementType(sig);
};

int32_t BindingsCCorSigUncompressElementTypeWithOutput(uint8_t* pData, int32_t* output) {
    auto sig = static_cast<PCCOR_SIGNATURE>(pData);
    return (int32_t)CorSigUncompressElementType(sig, (CorElementType*)output);
};

int32_t BindingsCCorSigUncompressToken(uint8_t* pData) {
    auto sig = static_cast<PCCOR_SIGNATURE>(pData);
    return (int32_t)CorSigUncompressToken(sig);
};


int32_t BindingsCCorSigUncompressTokenWithOutput(uint8_t* pData, int32_t* token) {
    auto sig = static_cast<PCCOR_SIGNATURE>(pData);
    return (int32_t)CorSigUncompressToken(sig, (mdToken*)token);
};

PCCOR_SIGNATURE BindingsSignatureConsumeType(PCCOR_SIGNATURE &signature) {
        PCCOR_SIGNATURE start = signature;

        CorElementType elementType{ CorSigUncompressElementType(signature) };
        switch (elementType) {
            case ELEMENT_TYPE_VOID:
            case ELEMENT_TYPE_BOOLEAN:
            case ELEMENT_TYPE_CHAR:
            case ELEMENT_TYPE_I1:
            case ELEMENT_TYPE_U1:
            case ELEMENT_TYPE_I2:
            case ELEMENT_TYPE_U2:
            case ELEMENT_TYPE_I4:
            case ELEMENT_TYPE_U4:
            case ELEMENT_TYPE_I8:
            case ELEMENT_TYPE_U8:
            case ELEMENT_TYPE_R4:
            case ELEMENT_TYPE_R8:
            case ELEMENT_TYPE_STRING:
                return start;

            case ELEMENT_TYPE_VALUETYPE:
                CorSigUncompressToken(signature);
                return start;

            case ELEMENT_TYPE_CLASS:
                CorSigUncompressToken(signature);
                return start;

            case ELEMENT_TYPE_OBJECT:
                return start;

            case ELEMENT_TYPE_SZARRAY:
                BindingsSignatureConsumeType(signature);
                return start;

            case ELEMENT_TYPE_VAR:
                CorSigUncompressData(signature);
                return start;

            case ELEMENT_TYPE_GENERICINST: {
                CorSigUncompressElementType(signature);
                CorSigUncompressToken(signature);

                ULONG genericArgumentsCount{ CorSigUncompressData(signature) };
                for (size_t i = 0; i < genericArgumentsCount; ++i) {
                    BindingsSignatureConsumeType(signature);
                }

                return start;
            }

            case ELEMENT_TYPE_BYREF:
                BindingsSignatureConsumeType(signature);
                return start;

            default:
                ASSERT_NOT_REACHED();
        }
}

void BindingsSignatureToString(IMetaDataImport2 *metadata, PCCOR_SIGNATURE signature, wstring &result) {
    CorElementType elementType{ CorSigUncompressElementType(signature) };
    switch (elementType) {
        case ELEMENT_TYPE_VOID:
            result += L"Void";
            break;

        case ELEMENT_TYPE_BOOLEAN:
            result += L"Boolean";
            break;

        case ELEMENT_TYPE_CHAR:
            result += L"Char16";
            break;

        case ELEMENT_TYPE_I1:
            result += L"Int8";
            break;

        case ELEMENT_TYPE_U1:
            result += L"UInt8";
            break;

        case ELEMENT_TYPE_I2:
            result += L"Int16";
            break;

        case ELEMENT_TYPE_U2:
            result += L"UInt16";
            break;

        case ELEMENT_TYPE_I4:
            result += L"Int32";
            break;

        case ELEMENT_TYPE_U4:
            result += L"UInt32";
            break;

        case ELEMENT_TYPE_I8:
            result += L"Int64";
            break;

        case ELEMENT_TYPE_U8:
            result += L"UInt64";
            break;

        case ELEMENT_TYPE_R4:
            result += L"Single";
            break;

        case ELEMENT_TYPE_R8:
            result += L"Double";
            break;

        case ELEMENT_TYPE_STRING:
            result += L"String";
            break;

        case ELEMENT_TYPE_VALUETYPE: {
            mdToken token{ CorSigUncompressToken(signature) };
            wstring className{ getTypeName(metadata, token) };
            if (className == L"System.Guid") {
                result += L"Guid";
            } else {
                result += className;
            }
            break;
        }

        case ELEMENT_TYPE_CLASS: {
            mdToken token{ CorSigUncompressToken(signature) };
            result += getTypeName(metadata, token);
            break;
        }

        case ELEMENT_TYPE_OBJECT:
            result += L"Object";
            break;

        case ELEMENT_TYPE_SZARRAY:
            BindingsSignatureToString(metadata, signature, result);
            result += L"[]";
            break;

        case ELEMENT_TYPE_VAR: {
            ULONG index{ CorSigUncompressData(signature) };
            result += L"Var!";
            result += to_wstring(index);
            break;
        }

        case ELEMENT_TYPE_GENERICINST: {
            CorElementType genericType{ CorSigUncompressElementType(signature) };
            ASSERT(genericType == ELEMENT_TYPE_CLASS);

            mdToken token{ CorSigUncompressToken(signature) };
            result += getTypeName(metadata, token);

            result += L'<';

            ULONG genericArgumentsCount{ CorSigUncompressData(signature) };
            for (size_t i = 0; i < genericArgumentsCount; ++i) {
                PCCOR_SIGNATURE type{ BindingsSignatureConsumeType(signature) };
                BindingsSignatureToString(metadata, type, result);

                if (i != genericArgumentsCount - 1) {
                    result += L", ";
                }
            }

            result += L'>';
            break;
        }

        case ELEMENT_TYPE_BYREF:
            result += L"ByRef ";
            BindingsSignatureToString(metadata, signature, result);
            break;

        default:
            ASSERT_NOT_REACHED();
    }
}


wstring BindingsSignatureToString(IMetaDataImport2* metadata, PCCOR_SIGNATURE signature) {
    wstring result;
    BindingsSignatureToString(metadata, signature, result);
    return result;
}

wstring getTypeName(IMetaDataImport2* metadata, mdToken token) {
    ASSERT(metadata);
    ASSERT(token != mdTokenNil);

    identifier nameData;
    ULONG nameLength{ 0 };

    switch (TypeFromToken(token)) {
    case mdtTypeDef:
        ASSERT_SUCCESS(metadata->GetTypeDefProps(token, nameData.data(), nameData.size(), &nameLength, nullptr, nullptr));
        break;

    case mdtTypeRef:
        ASSERT_SUCCESS(metadata->GetTypeRefProps(token, nullptr, nameData.data(), nameData.size(), &nameLength));
        break;

    default:
        ASSERT_NOT_REACHED();
    }

    wstring result{ nameData.data(), nameLength - 1 };
    return result;
}
