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

uint32_t BindingsCCorSigUncompressCallingConv(rust::Slice<const uint8_t> pData);

uint32_t BindingsCCorSigUncompressData(rust::Slice<const uint8_t> pData);

int32_t BindingsCCorSigUncompressElementType(rust::Slice<const uint8_t> pData);

int32_t BindingsCCorSigUncompressToken(rust::Slice<const uint8_t> pData);


#endif //WINDOWS_RUNTIME_BINDINGS_H
