//
// Created by fortu on 28/04/2023.
//

#include "bindings.h"
#include "rust/cxx.h"

int32_t BindingsCCorSigUncompressCallingConv(rust::Slice<const uint8_t> pData) {
    auto sig = (const PCCOR_SIGNATURE&)pData.data();
    return (int32_t)CorSigUncompressCallingConv(sig);
};

int32_t BindingsCCorSigUncompressData(rust::Slice<const uint8_t> pData) {
    auto sig = (const PCCOR_SIGNATURE&)pData.data();
    return (int32_t)CorSigUncompressData(sig);
};

int32_t BindingsCCorSigUncompressElementType(rust::Slice<const uint8_t> pData) {
    auto sig = (const PCCOR_SIGNATURE&)pData.data();
    return (int32_t)CorSigUncompressElementType(sig);
};

int32_t BindingsCCorSigUncompressToken(rust::Slice<const uint8_t> pData) {
    auto sig = (const PCCOR_SIGNATURE&)pData.data();
    return (int32_t)CorSigUncompressToken(sig);
};

int32_t BindingsCCorSigUncompressCallingConvRaw(uint8_t* pData) {
    auto sig = (PCCOR_SIGNATURE)pData;
    return (int32_t)CorSigUncompressCallingConv(sig);
};

int32_t BindingsCCorSigUncompressDataRaw(uint8_t* pData) {
    auto sig = (PCCOR_SIGNATURE)pData;
    return (int32_t)CorSigUncompressData(sig);
};

int32_t BindingsCCorSigUncompressElementTypeRaw(uint8_t* pData) {
    auto sig = (PCCOR_SIGNATURE)pData;
    return (int32_t)CorSigUncompressElementType(sig);
};

int32_t BindingsCCorSigUncompressTokenRaw(uint8_t* pData) {
    auto sig = (PCCOR_SIGNATURE)pData;
    return (int32_t)CorSigUncompressToken(sig);
};


