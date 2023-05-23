//
// Created by fortu on 28/04/2023.
//

#include "bindings.h"
#include "rust\cxx.h"
#include <comutil.h>
#include <iostream>

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

std::unique_ptr<GUID> GetGUIDForClassName(rust::Str data){
    CLSID clsid;

    std::string name(data.data(), data.size());


    const wchar_t* wideStr = _com_util::ConvertStringToBSTR(name.c_str());

    const wchar_t* className = L"Windows.Data.Json.JsonObject";


    auto hr = CLSIDFromProgID(className, &clsid);

    
    SysFreeString(const_cast<wchar_t*>(wideStr));

    if (SUCCEEDED(hr)) {
        // GUID obtained successfully
        // Access clsid as needed
        // ...

        // Print the GUID as a string
        wchar_t guidString[40];
        StringFromGUID2(clsid, guidString, sizeof(guidString) / sizeof(guidString[0]));
        std::wcout << L"GUID: " << guidString << std::endl;
    }


    return std::make_unique<GUID>(static_cast<GUID>(clsid));
}