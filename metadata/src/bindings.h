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

std::unique_ptr<GUID> GetGUID(const uint8_t* data);

uint32_t GetData1(const GUID& guid);

uint16_t GetData2(const GUID& guid);

uint16_t GetData3(const GUID& guid);

rust::Slice<const uint8_t> GetData4(const GUID& guid);


#endif //WINDOWS_RUNTIME_BINDINGS_H