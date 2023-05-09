//
// Created by fortu on 28/04/2023.
//

#ifndef WINDOWS_RUNTIME_BINDINGS_H
#define WINDOWS_RUNTIME_BINDINGS_H

#include <windows.h>

#include <wrl.h>
#include <wrl/wrappers/corewrappers.h>
#include <wrl/client.h>

#include <array>
#include <string>
#include <memory>
#include <comdef.h>
#include <Rometadataresolution.h>
#include <cor.h>
#include "rust/cxx.h"


// This limit is only for C# and not for the CLI, but it is good enough
const size_t MAX_IDENTIFIER_LENGTH{ 511 };
using identifier = std::array<wchar_t, MAX_IDENTIFIER_LENGTH + 1>;

#define NO_RETURN __declspec(noreturn)

NO_RETURN void CRASH_IMPL();

#define CRASH(hresult)                                                        \
    do {                                                                      \
        if (IsDebuggerPresent()) {                                            \
            OutputDebugString(_com_error{ hresult, nullptr }.ErrorMessage()); \
            __debugbreak();                                                   \
        }                                                                     \
                                                                              \
        CRASH_IMPL();                                                         \
    } while (0)

#define NOT_IMPLEMENTED() \
    do {                  \
        CRASH(E_NOTIMPL); \
    } while (0)

#ifdef _DEBUG
        #define ASSERT(booleanExpression)           \
    do {                                    \
        if (!(booleanExpression)) {         \
            CRASH(ERROR_ASSERTION_FAILURE); \
        }                                   \
    } while (0)
#else
#define ASSERT(booleanExpression)
#endif

#ifdef _DEBUG
#define ASSERT_NOT_REACHED() ASSERT(false)
#else
#define ASSERT_NOT_REACHED()
#endif

#ifdef _DEBUG
        #define ASSERT_SUCCESS(hr)       \
    do {                         \
        HRESULT __hresult{ hr }; \
        if (FAILED(__hresult)) { \
            CRASH(__hresult);    \
        }                        \
    } while (0)
#else
#define ASSERT_SUCCESS(hr) ((void)hr)
#endif

#ifdef _DEBUG
        void DEBUG_LOG(_Printf_format_string_ const wchar_t* format, ...);
#else
#define DEBUG_LOG(format, ...)
#endif

using c_void = void;

int32_t BindingsCCorSigUncompressCallingConv(uint8_t* pData);

int32_t BindingsCCorSigUncompressData(uint8_t* pData);

int32_t BindingsCCorSigUncompressDataWithOutput(uint8_t* pData, uint32_t* output);

int32_t BindingsCCorSigUncompressElementType(uint8_t* pData);

int32_t BindingsCCorSigUncompressElementTypeWithOutput(uint8_t* pData, int32_t* output);

int32_t BindingsCCorSigUncompressToken(uint8_t* pData);

int32_t BindingsCCorSigUncompressTokenWithOutput(uint8_t* pData, int32_t* token);

PCCOR_SIGNATURE BindingsSignatureConsumeType(PCCOR_SIGNATURE& signature);

void BindingsSignatureToString(IMetaDataImport2* metadata, PCCOR_SIGNATURE signature, std::wstring& result);

std::wstring BindingsSignatureToString(IMetaDataImport2* metadata, PCCOR_SIGNATURE signature);

std::wstring getTypeName(IMetaDataImport2* metadata, mdToken token);



#endif //WINDOWS_RUNTIME_BINDINGS_H
