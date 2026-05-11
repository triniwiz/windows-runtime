//
// Created by fortu on 28/04/2023.
//

#include "bindings.h"


void GetMethod(IUnknown *iface, size_t index, c_void **method) {
    Microsoft::WRL::ComPtr<IInspectable> factory;
    factory.Attach(reinterpret_cast<IInspectable *>(iface));

    void **vtable = *reinterpret_cast<void ***>(factory.Get());
    *method = vtable[index];
}

rust::String GUIDToString(uint32_t Data1, uint16_t Data2, uint16_t Data3,
                           rust::Slice<const uint8_t> Data4) {
    GUID guid;
    guid.Data1 = Data1;
    guid.Data2 = Data2;
    guid.Data3 = Data3;
    std::memcpy(&guid.Data4, Data4.data(), 8);

    wchar_t guidString[40];
    StringFromGUID2(guid, guidString, sizeof(guidString) / sizeof(guidString[0]));

    std::wstring buf(guidString);
    auto data = reinterpret_cast<const char16_t *>(buf.c_str());
    return rust::String(data, buf.size());
}

void QueryInterface(size_t index, c_void *factory, uint32_t Data1, uint16_t Data2, uint16_t Data3,
                    rust::Slice<const uint8_t> Data4, c_void *activation_factory, c_void **func) {
    Microsoft::WRL::ComPtr<IUnknown> classFactory(static_cast<IUnknown *>(factory));
    Microsoft::WRL::ComPtr<IUnknown> activationFactory(static_cast<IUnknown *>(activation_factory));

    GUID guid;
    guid.Data1 = Data1;
    guid.Data2 = Data2;
    guid.Data3 = Data3;
    std::memcpy(&guid.Data4, Data4.data(), 8);

    classFactory->QueryInterface(guid,
                                 reinterpret_cast<void **>(activationFactory.GetAddressOf()));

    void **vtable = *reinterpret_cast<void ***>(activationFactory.Get());
    *func = vtable[index];
}

void *OpenMetadataScope(rust::Str path_utf8) {
    // Create a metadata dispenser from mscoree.dll.
    // This works for any PE/COFF file carrying CLI metadata (.dll, .winmd, .exe).
    IMetaDataDispenserEx *pDispenser = nullptr;
    HRESULT hr = CoCreateInstance(
            CLSID_CorMetaDataDispenser,
            nullptr,
            CLSCTX_INPROC_SERVER,
            IID_IMetaDataDispenserEx,
            reinterpret_cast<void **>(&pDispenser)
    );
    if (FAILED(hr) || !pDispenser) {
        return nullptr;
    }

    // Convert UTF-8 path to wide string for the Win32 API.
    std::string narrow(path_utf8.data(), path_utf8.size());
    int wlen = MultiByteToWideChar(CP_UTF8, 0, narrow.c_str(), -1, nullptr, 0);
    if (wlen <= 0) {
        pDispenser->Release();
        return nullptr;
    }
    std::wstring wide(static_cast<size_t>(wlen), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, narrow.c_str(), -1, &wide[0], wlen);

    IMetaDataImport2 *pImport = nullptr;
    hr = pDispenser->OpenScope(wide.c_str(), ofRead, IID_IMetaDataImport2,
                               reinterpret_cast<IUnknown **>(&pImport));
    pDispenser->Release();

    return FAILED(hr) ? nullptr : static_cast<void *>(pImport);
}
