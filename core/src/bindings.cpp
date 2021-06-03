#include "headers.h"


typedef HRESULT (__cdecl * ProcRoGetMetaDataFile)(const HSTRING name, IMetaDataDispenserEx *, HSTRING *, IMetaDataImport2 **, mdTypeDef *typeDefToken);


extern "C" HRESULT Rometadataresolution_RoGetMetaDataFile(const HSTRING name, void* metaDataDispenser, void* metaDataFilePath, void** metaDataImport, mdTypeDef *typeDefToken){
    auto dispenser = reinterpret_cast<IMetaDataDispenserEx *>(metaDataDispenser);
    auto filePath =  reinterpret_cast<HSTRING *>(metaDataFilePath);
    auto import = (IMetaDataImport2 **)(metaDataImport);

    auto proc = (ProcRoGetMetaDataFile)GetProcAddress(
            LoadLibraryA("api-ms-win-ro-typeresolution-l1-1-0.dll"),
            "RoGetMetaDataFile"
            );

    return proc(name, dispenser, filePath, import, typeDefToken);
}

extern "C" void IMetaDataImport2_GetTypeDefProps(void* metadata, uint32_t mdTypeDef, void* szTypeDef, ULONG cchTypeDef, ULONG * pchTypeDef , DWORD * pdwTypeDefFlags, mdToken * ptkExtends) {
    auto meta = static_cast<IMetaDataImport2*>(metadata);
    auto data = reinterpret_cast<wchar_t *>(szTypeDef);
    meta->GetTypeDefProps(mdTypeDef, data, cchTypeDef, pchTypeDef, pdwTypeDefFlags, ptkExtends);
}

extern "C" void IMetaDataImport2_GetTypeDefPropsNameSize(void* metadata, uint32_t token, ULONG * pchTypeDef) {
    auto meta = static_cast<IMetaDataImport2*>(metadata);
    auto size = reinterpret_cast<ULONG *>(&pchTypeDef);
    auto result = meta->GetTypeDefProps(token, 0, 0, size, 0, 0);
}


extern "C" void IMetaDataImport2_EnumInterfaceImpls(void* meta, mdToken token) {
    auto metadata = reinterpret_cast<IMetaDataImport2*>(meta);
    HCORENUM enumerator{ nullptr };
    ULONG count{ 0 };
    std::array<mdInterfaceImpl, 2048> tokens;
    assert(metadata->EnumInterfaceImpls(&enumerator, token, tokens.data(), tokens.size(), &count));
    assert(count < tokens.size() - 1);
    metadata->CloseEnum(enumerator);
    std::cout << "count " << count << "\n";

    //vector<unique_ptr<const InterfaceDeclaration>> result;
    for (size_t i = 0; i < count; ++i) {
        mdToken interfaceToken{ mdTokenNil };
        assert(metadata->GetInterfaceImplProps(tokens[i], nullptr, &interfaceToken));

        //result.push_back(DeclarationFactory::makeInterfaceDeclaration(metadata, interfaceToken));
    }


  //  return result;


}


extern "C" ULONG32 Enums_TypeFromToken(mdToken token) {
    return TypeFromToken(token);
}


extern "C" ULONG Helpers_Get_Type_Name(void* meta , mdToken token,uint16_t * nameData, size_t nameSize) {
    auto metadata = reinterpret_cast<IMetaDataImport2*>(meta);
    ULONG nameLength{ 0 };
    auto data = reinterpret_cast<LPWSTR>(nameData);

    switch (TypeFromToken(token)) {
    case CorTokenType::mdtTypeDef: {
        metadata->GetTypeDefProps(token, data, nameSize, &nameLength, nullptr, nullptr);
        break;
    }
    case CorTokenType::mdtTypeRef: {
        metadata->GetTypeRefProps(token, nullptr, data, nameSize, &nameLength);
        break;
    }
    default:
        assert();
        break;
    }

    return nameLength - 1;
}

extern "C" BOOL Helpers_IsTdPublic(DWORD value){
    return IsTdPublic(value);
}

extern "C" BOOL Helpers_IsTdSpecialName(DWORD value){
    return IsTdSpecialName(value);
}