//
// Created by fortu on 28/04/2023.
//

#include "bindings.h"
#include "rust\cxx.h"
#include <comutil.h>
#include <iostream>

rust::String GUIDToString(uint32_t Data1, uint16_t Data2, uint16_t Data3, rust::Slice<const uint8_t> Data4) {
    GUID guid;
    guid.Data1 = Data1;
    guid.Data2 = Data2;
    guid.Data3 = Data3;
    std::memcpy(&guid.Data4, Data4.data(), 8);

    wchar_t guidString[40];
    StringFromGUID2(guid, guidString, sizeof(guidString) / sizeof(guidString[0]));

    std::wstring buf(guidString);
    auto data = (char16_t*)buf.c_str();
    auto size = buf.size();
    return rust::String(data, size);
}

void QueryInterface(size_t index, c_void* factory, uint32_t Data1, uint16_t Data2, uint16_t Data3, rust::Slice<const uint8_t> Data4,  c_void* activation_factory, c_void** func) {
    Microsoft::WRL::ComPtr<IUnknown> classFactory(static_cast<IUnknown*>(factory));
    Microsoft::WRL::ComPtr<IUnknown> activationFactory(static_cast<IUnknown*>(activation_factory));

    GUID guid;
    guid.Data1 = Data1;
    guid.Data2 = Data2;
    guid.Data3 = Data3;

    std::memcpy(&guid.Data4, Data4.data(), 8);

    classFactory->QueryInterface(guid, reinterpret_cast<void**>(activationFactory.GetAddressOf()));

    void** vtable = *reinterpret_cast<void***>(activationFactory.Get());

    void* fun = vtable[index];

    *func = fun;
}

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