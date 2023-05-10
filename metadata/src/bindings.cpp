//
// Created by fortu on 28/04/2023.
//

#include "bindings.h"
#include "rust/cxx.h"

std::unique_ptr <GUID> GetGUID(const uint8_t *data) {
    GUID guid(*reinterpret_cast<const GUID *>(data));
    return std::make_unique<GUID>(guid);
}

uint32_t GetData1(const GUID& guid) {
    return guid.Data1;
}

uint16_t GetData2(const GUID& guid) {
    return guid.Data2;
}

uint16_t GetData3(const GUID& guid) {
    return guid.Data3;
}

rust::Slice<const uint8_t> GetData4(const GUID& guid) {
    auto data = guid.Data4; 
    return rust::Slice<const uint8_t>(data, 8);
}