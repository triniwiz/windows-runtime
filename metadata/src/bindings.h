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
#include <cor.h>
#include "rust/cxx.h"

#include <comutil.h>
#include <iostream>

using c_void = void;

// Vtable helpers — cannot be cleanly expressed in safe Rust without UB.
void GetMethod(IUnknown *iface, size_t index, c_void **method);

void QueryInterface(size_t index, c_void *factory, uint32_t Data1, uint16_t Data2, uint16_t Data3,
                    rust::Slice<const uint8_t> Data4, c_void *activation_factory, c_void **func);

// Opens a metadata scope from a file path using IMetaDataDispenserEx::OpenScope.
// Returns an AddRef'd IMetaDataImport2* cast to void*, or nullptr on failure.
// The caller is responsible for Release().
void *OpenMetadataScope(rust::Str path_utf8);

rust::String GUIDToString(uint32_t Data1, uint16_t Data2, uint16_t Data3, rust::Slice<const uint8_t> Data4);

#endif //WINDOWS_RUNTIME_BINDINGS_H
