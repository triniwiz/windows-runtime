//
// Created by fortu on 28/04/2023.
//

#ifndef WINDOWS_RUNTIME_BINDINGS_H
#define WINDOWS_RUNTIME_BINDINGS_H

#include <windows.h>
#include <objbase.h>
#include <combaseapi.h>

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

#include <comutil.h>
#include <iostream>

using c_void = void;

void PrintVtableNames(IUnknown * iface);

void GetMethod(IUnknown *iface, size_t index, c_void **method);

std::unique_ptr <GUID> GetGUID(const uint8_t *data);

uint32_t GetData1(const GUID &guid);

uint16_t GetData2(const GUID &guid);

uint16_t GetData3(const GUID &guid);

rust::Slice<const uint8_t> GetData4(const GUID &guid);

std::unique_ptr <GUID> GetGUIDForClassName(rust::Str data);

void QueryInterface(size_t index, c_void *factory, uint32_t Data1, uint16_t Data2, uint16_t Data3,
                    rust::Slice<const uint8_t> Data4, c_void *activation_factory, c_void **func);

rust::String GUIDToString(uint32_t Data1, uint16_t Data2, uint16_t Data3, rust::Slice<const uint8_t> Data4);

#endif //WINDOWS_RUNTIME_BINDINGS_H