#include "headers.h"


typedef HRESULT (__cdecl *ProcRoGetMetaDataFile)(const HSTRING name, IMetaDataDispenserEx *, HSTRING *,
                                                 IMetaDataImport2 **, mdTypeDef *typeDefToken);


typedef HRESULT(__cdecl *ProcRoParseTypeName)(const HSTRING typeName, DWORD *partsCount, HSTRING **typeNameParts);

typedef HRESULT(__cdecl *ProcRoGetParameterizedTypeInstanceIID)(UINT32 nameElementCount,
                                                                PCWSTR *nameElements,
                                                                const IRoMetaDataLocator &metaDataLocator,
                                                                GUID *iid,
                                                                ROPARAMIIDHANDLE *pExtra);


extern "C" HRESULT
Rometadataresolution_RoGetMetaDataFile(const HSTRING name, void *metaDataDispenser, void *metaDataFilePath,
                                       void **metaDataImport, mdTypeDef *typeDefToken) {
    auto dispenser = reinterpret_cast<IMetaDataDispenserEx *>(metaDataDispenser);
    auto filePath = reinterpret_cast<HSTRING *>(metaDataFilePath);
    auto import = (IMetaDataImport2 * *)(metaDataImport);

    /* auto proc = (ProcRoGetMetaDataFile)GetProcAddress(
             LoadLibraryA("api-ms-win-ro-typeresolution-l1-1-0.dll"),
             "RoGetMetaDataFile");

     return proc(name, dispenser, filePath, import, typeDefToken);
     */
    return RoGetMetaDataFile(name, dispenser, filePath, import, typeDefToken);
}


extern "C" HRESULT
Rometadataresolution_RoParseTypeName(const HSTRING typeName, DWORD *partsCount, HSTRING **typeNameParts) {
    /*auto proc = (ProcRoParseTypeName)GetProcAddress(
        LoadLibraryA("api-ms-win-ro-typeresolution-l1-1-0.dll"),
        "RoParseTypeName");
  
    return proc(typeName, partsCount, typeNameParts);*/
    return RoParseTypeName(typeName, partsCount, typeNameParts);
}



extern "C" HRESULT Rometadataresolution_RoGetParameterizedTypeInstanceIID(UINT32 nameElementCount,
                                                                          PCWSTR *nameElements,
                                                                          HRESULT(*fn)(PCWSTR name, IRoSimpleMetaDataBuilder & builder),
                                                                          GUID *iid,
                                                                          ROPARAMIIDHANDLE *pExtra) {



    auto locatorImpl{ [&](PCWSTR name, IRoSimpleMetaDataBuilder& builder) {
        return fn(name, builder);
    } };


    //auto const locator = locatorImpl;

    return RoGetParameterizedTypeInstanceIID(nameElementCount, nameElements, Ro::Locator(locatorImpl), iid, pExtra);
}

//typedef HRESULT(void* RoLocator)(PCWSTR name, iro_simple_meta_data_builder& builder);


//typedef HRESULT(*RoLocator)(PCWSTR name, iro_simple_meta_data_builder& builder);

struct _locator {
    Ro::detail::_Locator<HRESULT(__cdecl *)(PCWSTR, IRoSimpleMetaDataBuilder &)> locator;
};


extern "C" void
Rometadataresolution_Ro_Locator(HRESULT(*fn)(PCWSTR name, IRoSimpleMetaDataBuilder &builder), Locator *locator) {
    /*auto proc = (ProcRoGetParameterizedTypeInstanceIID)GetProcAddress(
        LoadLibraryA("api-ms-win-ro-typeresolution-l1-1-0.dll"),
        "ro_get_parameterized_type_instance_iid");

    return proc(nameElementCount, nameElements, metaDataLocator, iid, pExtra); */
    locator->locator = Ro::Locator(fn);
}

/*
extern "C" void
Rometadataresolution_RoResolveNamespace(const HSTRING name,
    const HSTRING windowsMetaDataDir,
    const DWORD   packageGraphDirsCount,
    const HSTRING * packageGraphDirs,
    DWORD * metaDataFilePathsCount,
    HSTRING * *metaDataFilePaths,
    DWORD * subNamespacesCount,
    HSTRING * *subNamespaces) {
    RoResolveNamespace()
}


*/

extern "C" HRESULT IMetaDataImport2_GetMethodProps(IMetaDataImport2 *metadata,
                                                   mdMethodDef tkMethodDef,
                                                   mdTypeDef *ptkClass,
                                                   LPWSTR szMethod,
                                                   ULONG cchMethod,
                                                   ULONG *pchMethod,
                                                   DWORD *pdwAttr,
                                                   PCCOR_SIGNATURE *ppvSigBlob,
                                                   ULONG *pcbSigBlob,
                                                   ULONG *pulCodeRVA,
                                                   DWORD *pdwImplFlags) {
    return metadata->GetMethodProps(tkMethodDef, ptkClass, szMethod, cchMethod, pchMethod, pdwAttr, ppvSigBlob, pcbSigBlob,
                                pulCodeRVA, pdwImplFlags);
}

extern "C" HRESULT IMetaDataImport2_GetPropertyProps(IMetaDataImport2 *metadata, mdProperty prop,
                                                     mdTypeDef *pClass,
                                                     LPCWSTR szProperty,
                                                     ULONG cchProperty,
                                                     ULONG *pchProperty,
                                                     DWORD *pdwPropFlags,
                                                     PCCOR_SIGNATURE *ppvSig,
                                                     ULONG *pbSig,
                                                     DWORD *pdwCPlusTypeFlag,
                                                     UVCP_CONSTANT *ppDefaultValue,
                                                     ULONG *pcchDefaultValue,
                                                     mdMethodDef *pmdSetter,
                                                     mdMethodDef *pmdGetter,
                                                     mdMethodDef *rmdOtherMethod,
                                                     ULONG cMax,
                                                     ULONG *pcOtherMethod) {
    return metadata->GetPropertyProps(prop, pClass, szProperty, cchProperty, pchProperty, pdwPropFlags, ppvSig, pbSig,
                                  pdwCPlusTypeFlag,
                                  ppDefaultValue, pcchDefaultValue, pmdSetter, pmdGetter, rmdOtherMethod, cMax,
                                  pcOtherMethod);
}



extern "C" HRESULT
IMetaDataImport2_GetFieldProps(IMetaDataImport2 *metadata,
                               mdFieldDef mb,
                               mdTypeDef *pClass,
                               LPWSTR szField,
                               ULONG cchField,
                               ULONG *pchField,
                               DWORD *pdwAttr,
                               PCCOR_SIGNATURE *ppvSigBlob,
                               ULONG *pcbSigBlob,
                               DWORD *pdwCPlusTypeFlag,
                               UVCP_CONSTANT *ppValue,
                               ULONG *pcchValue) {
    
    return metadata->GetFieldProps(mb, pClass, szField, cchField, pchField, pdwAttr, ppvSigBlob, pcbSigBlob,
                               pdwCPlusTypeFlag, ppValue, pcchValue);
}

extern "C" HRESULT
IMetaDataImport2_GetTypeDefProps(IMetaDataImport2 *metadata, uint32_t mdTypeDef, LPWSTR szTypeDef, ULONG cchTypeDef,
                                 ULONG *pchTypeDef, DWORD *pdwTypeDefFlags, mdToken *ptkExtends) {
    
    return metadata->GetTypeDefProps(mdTypeDef, szTypeDef, cchTypeDef, pchTypeDef, pdwTypeDefFlags, ptkExtends);
}

extern "C" HRESULT IMetaDataImport2_GetTypeDefPropsNameSize(IMetaDataImport2 *metadata, uint32_t token, ULONG *pchTypeDef) {
    
    auto size = reinterpret_cast<ULONG *>(&pchTypeDef);
    return metadata->GetTypeDefProps(token, 0, 0, size, 0, 0);
}

extern "C" HRESULT IMetaDataImport2_EnumParams(IMetaDataImport2 *metadata, HCORENUM *phEnum,
                                               mdMethodDef mb,
                                               mdParamDef *rParams,
                                               ULONG cMax,
                                               ULONG *pcTokens) {
    
    return metadata->EnumParams(phEnum, mb, rParams, cMax, pcTokens);
}

extern "C" void IMetaDataImport2_CloseEnum(IMetaDataImport2 *metadata, HCORENUM phEnum) {
    
    metadata->CloseEnum(phEnum);
}



extern "C" HRESULT IMetaDataImport2_GetParamProps(IMetaDataImport2 *metadata,
                                                  mdParamDef tk,
                                                  mdMethodDef *pmd,
                                                  ULONG *pulSequence,
                                                  LPWSTR szName,
                                                  ULONG cchName,
                                                  ULONG *pchName,
                                                  DWORD *pdwAttr,
                                                  DWORD *pdwCPlusTypeFlag,
                                                  UVCP_CONSTANT *ppValue,
                                                  ULONG *pcchValue) {
    
    return metadata->GetParamProps(tk, pmd, pulSequence, szName, cchName, pchName, pdwAttr, pdwCPlusTypeFlag, ppValue,
                               pcchValue);
}

extern "C" HRESULT IMetaDataImport2_GetCustomAttributeByName(IMetaDataImport2 *metadata,
                                                             mdToken tkObj,
                                                             LPCWSTR szName,
                                                             const void **ppData,
                                                             ULONG *pcbData) {
    
    return metadata->GetCustomAttributeByName(tkObj, szName, ppData, pcbData);
}

extern "C" HRESULT IMetaDataImport2_EnumInterfaceImpls(IMetaDataImport2 *metadata, HCORENUM *phEnum,
                                                       mdTypeDef td,
                                                       mdInterfaceImpl *rImpls,
                                                       ULONG cMax,
                                                       ULONG *pcImpls) {
   
    return metadata->EnumInterfaceImpls(phEnum, td, rImpls, cMax, pcImpls);
}

extern "C" HRESULT IMetaDataImport2_GetTypeRefProps(IMetaDataImport2 *metadata, mdTypeRef tr,
                                                    mdToken *ptkResolutionScope,
                                                    LPWSTR szName,
                                                    ULONG cchName,
                                                    ULONG *pchName) {
   
    return metadata->GetTypeRefProps(tr, ptkResolutionScope, szName, cchName, pchName);
}

extern "C" HRESULT IMetaDataImport2_FindMethod(IMetaDataImport2 *metadata, mdTypeDef td,
                                               LPCWSTR szName,
                                               PCCOR_SIGNATURE pvSigBlob,
                                               ULONG cbSigBlob,
                                               mdMethodDef *pmb) {
   
    return metadata->FindMethod(td, szName, pvSigBlob, cbSigBlob, pmb);
}

extern "C" HRESULT IMetaDataImport2_EnumGenericParams(IMetaDataImport2 *metadata, HCORENUM *phEnum,
                                                      mdToken tk,
                                                      mdGenericParam rGenericParams[],
                                                      ULONG cMax,
                                                      ULONG *pcGenericParams) {
   
    return metadata->EnumGenericParams(phEnum, tk, rGenericParams, cMax, pcGenericParams);
}


extern "C" HRESULT IMetaDataImport2_CountEnum(IMetaDataImport2 *metadata, HCORENUM hEnum, ULONG *pulCount) {
   
    return metadata->CountEnum(hEnum, pulCount);
}

extern "C" HRESULT IMetaDataImport2_GetTypeSpecFromToken(IMetaDataImport2 *metadata, mdTypeSpec typespec,
                                                         PCCOR_SIGNATURE *ppvSig,
                                                         ULONG *pcbSig) {
   
    return metadata->GetTypeSpecFromToken(typespec, ppvSig, pcbSig);
}


extern "C" HRESULT IMetaDataImport2_EnumFields(IMetaDataImport2 *metadata, HCORENUM *phEnum,
                                               mdTypeDef tkTypeDef,
                                               mdFieldDef rgFields[],
                                               ULONG cMax,
                                               ULONG *pcTokens) {
   
    return metadata->EnumFields(phEnum, tkTypeDef, rgFields, cMax, pcTokens);
}


extern "C" HRESULT IMetaDataImport2_EnumMethodsWithName(IMetaDataImport2 *metadata, HCORENUM *phEnum,
                                                        mdTypeDef tkTypeDef,
                                                        LPCWSTR szName,
                                                        mdMethodDef rgMethods[],
                                                        ULONG cMax,
                                                        ULONG *pcTokens) {
   
    return metadata->EnumMethodsWithName(phEnum, tkTypeDef, szName, rgMethods, cMax, pcTokens);
}

extern "C" HRESULT IMetaDataImport2_GetInterfaceImplProps(IMetaDataImport2 *metadata, mdInterfaceImpl tkInterfaceImpl,
                                                          mdTypeDef *ptkClass,
                                                          mdToken *ptkIface) {
   
    return metadata->GetInterfaceImplProps(tkInterfaceImpl, ptkClass, ptkIface);
}

extern "C" HRESULT IMetaDataImport2_EnumMethods(IMetaDataImport2 *metadata, HCORENUM *phEnum,
                                                mdTypeDef tkTypeDef,
                                                mdMethodDef rgMethods[],
                                                ULONG cMax,
                                                ULONG *pcTokens) {
   
    return metadata->EnumMethods(phEnum, tkTypeDef, rgMethods, cMax, pcTokens);
}


extern "C" HRESULT IMetaDataImport2_EnumProperties(IMetaDataImport2 *metadata, HCORENUM *phEnum,
                                                   mdTypeDef tkTypDef,
                                                   mdProperty rgProperties[],
                                                   ULONG cMax,
                                                   ULONG *pcProperties) {
   
    return metadata->EnumProperties(phEnum, tkTypDef, rgProperties, cMax, pcProperties);
}

extern "C" HRESULT IMetaDataImport2_EnumEvents(IMetaDataImport2 *metadata, HCORENUM *phEnum,
                                               mdTypeDef tkTypDef,
                                               mdEvent rgEvents[],
                                               ULONG cMax,
                                               ULONG *pcEvents) {
   
    return metadata->EnumEvents(phEnum, tkTypDef, rgEvents, cMax, pcEvents);
}

extern "C" HRESULT IMetaDataImport2_GetEventProps(IMetaDataImport2 *metadata, mdEvent ev,
                                                  mdTypeDef *pClass,
                                                  LPCWSTR szEvent,
                                                  ULONG cchEvent,
                                                  ULONG *pchEvent,
                                                  DWORD *pdwEventFlags,
                                                  mdToken *ptkEventType,
                                                  mdMethodDef *pmdAddOn,
                                                  mdMethodDef *pmdRemoveOn,
                                                  mdMethodDef *pmdFire,
                                                  mdMethodDef rmdOtherMethod[],
                                                  ULONG cMax,
                                                  ULONG *pcOtherMethod) {
   
    return metadata->GetEventProps(ev, pClass, szEvent, cchEvent, pchEvent, pdwEventFlags, ptkEventType, pmdAddOn,
                                   pmdRemoveOn, pmdFire, rmdOtherMethod, cMax, pcOtherMethod);
}


extern "C" HRESULT IMetaDataImport2_FindField(IMetaDataImport2* metadata, mdTypeDef   td,
    LPCWSTR     szName,                 
    PCCOR_SIGNATURE pvSigBlob,        
    ULONG       cbSigBlob,
    mdFieldDef * pmb) {
    return metadata->FindField(td, szName, pvSigBlob, cbSigBlob, pmb);
}

extern "C" HRESULT IMetaDataImport2_GetMemberRefProps(IMetaDataImport2* metadata, mdMemberRef     tkMemberRef,
    mdToken * ptk,
    LPWSTR          szMember,
    ULONG           cchMember,
    ULONG * pchMember,
    PCCOR_SIGNATURE * ppvSigBlob,
    ULONG * pcbSigBlob) {
    return metadata->GetMemberRefProps(tkMemberRef, ptk, szMember, cchMember, pchMember, ppvSigBlob, pcbSigBlob);
}

extern "C" HRESULT IMetaDataImport2_GetCustomAttributeProps(IMetaDataImport2* metadata, mdCustomAttribute cv,
    mdToken * ptkObj,
    mdToken * ptkType,
    const BYTE * *ppBlob,
    ULONG * pcbBlob) {
    auto blob = reinterpret_cast<const void**>(ppBlob);
    return metadata->GetCustomAttributeProps(cv, ptkObj, ptkType, blob, pcbBlob);
}

extern "C" HRESULT IMetaDataImport2_EnumCustomAttributes(IMetaDataImport2 * metadata, HCORENUM * phEnum,
    mdToken              tk,
    mdToken              tkType,
    mdCustomAttribute rgCustomAttributes[],
    ULONG                cMax,
    ULONG * pcCustomAttributes) {
    return metadata->EnumCustomAttributes(phEnum, tk, tkType, rgCustomAttributes, cMax, pcCustomAttributes);
}

extern "C" HRESULT IMetaDataImport2_FindTypeDefByName(IMetaDataImport2 * metadata, LPCWSTR   szTypeDef,
    mdToken   tkEnclosingClass,
    mdTypeDef * ptkTypeDef) {
    return metadata->FindTypeDefByName(szTypeDef, tkEnclosingClass, ptkTypeDef);
}




extern "C" HRESULT IRoSimpleMetaDataBuilder_SetRuntimeClassSimpleDefault(IRoSimpleMetaDataBuilder* builder, PCWSTR     name,
    PCWSTR     defaultInterfaceName,
    const GUID * defaultInterfaceIID) {
    return builder->SetRuntimeClassSimpleDefault(name, defaultInterfaceName, defaultInterfaceIID);
}

extern "C" HRESULT IRoSimpleMetaDataBuilder_SetWinRtInterface(IRoSimpleMetaDataBuilder * builder, GUID iid) {
    return builder->SetWinRtInterface(iid);
}

extern "C" HRESULT IRoSimpleMetaDataBuilder_SetParameterizedInterface(IRoSimpleMetaDataBuilder* builder, GUID  piid,
    UINT32 numArgs) {
    return builder->SetParameterizedInterface(piid,numArgs);
}

extern "C" HRESULT IRoSimpleMetaDataBuilder_SetEnum(IRoSimpleMetaDataBuilder * builder, PCWSTR name,
    PCWSTR baseType) {
    return builder->SetEnum(name, baseType);
}


extern "C" HRESULT IRoSimpleMetaDataBuilder_SetStruct(IRoSimpleMetaDataBuilder * builder, PCWSTR       name,
    UINT32       numFields,
    const PCWSTR * fieldTypeNames) {
    return builder->SetStruct(name, numFields, fieldTypeNames);
}


extern "C" HRESULT IRoSimpleMetaDataBuilder_SetDelegate(IRoSimpleMetaDataBuilder * builder, GUID iid) {
    return builder->SetDelegate(iid);
}


extern "C" HRESULT IRoSimpleMetaDataBuilder_SetParameterizedDelegate(IRoSimpleMetaDataBuilder * builder, GUID   piid,
    UINT32 numArgs) {
    return builder->SetParameterizedDelegate(piid, numArgs);
}



extern "C" ULONG32 Enums_TypeFromToken(mdToken token) {
    return TypeFromToken(token);
}


extern "C" ULONG Helpers_Get_Type_Name(IMetaDataImport2 *metadata, mdToken token, uint16_t *nameData, ULONG nameSize) {
   
    ULONG nameLength{0};
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
            assert(false);
            break;
    }

    return nameLength - 1;
}

extern "C" BOOL Helpers_IsTdPublic(DWORD value) {
    return IsTdPublic(value);
}

extern "C" BOOL Helpers_IsTdSpecialName(DWORD value) {
    return IsTdSpecialName(value);
}

extern "C" ULONG Helpers_CorSigUncompressCallingConv(PCCOR_SIGNATURE pData) {
    return CorSigUncompressCallingConv(pData);
}


extern "C" ULONG Helpers_CorSigUncompressData(PCCOR_SIGNATURE pData) {
    return CorSigUncompressData(pData);
}

extern "C" CorElementType Helpers_CorSigUncompressElementType(PCCOR_SIGNATURE pData) {
    return CorSigUncompressElementType(pData);
}

extern "C" mdToken Helpers_CorSigUncompressToken(PCCOR_SIGNATURE pData) {
    return CorSigUncompressToken(pData);
}

extern "C" size_t Helpers_to_wstring(ULONG index, wchar_t *text) {
    using namespace std;
    auto str = to_wstring(index);
    auto txt = std::wstring(text);
    txt += str;
    wcscpy(text, txt.c_str());
    return str.length() - 1;
}

extern "C" BOOL Helpers_IsMdPublic(DWORD value) {
    return IsMdPublic(value);
}

extern "C" BOOL Helpers_IsMdFamily(DWORD value) {
    return IsMdFamily(value);
}

extern "C" BOOL Helpers_IsMdFamORAssem(DWORD value) {
    return IsMdFamORAssem(value);
}

extern "C" BOOL Helpers_IsMdSpecialName(DWORD value) {
    return IsMdSpecialName(value);
}

extern "C" BOOL Helpers_IsMdStatic(DWORD value) {
    return IsMdStatic(value);
}

extern "C" BOOL Helpers_IsMdFinal(DWORD value) {
    return IsMdFinal(value);
}

extern "C" BOOL Helpers_IsMdInstanceInitializerW(DWORD flags, const wchar_t *nameData) {
    return IsMdInstanceInitializerW(flags, nameData);
}

extern "C" BOOL Helpers_IsPrSpecialName(DWORD value) {
    return IsPrSpecialName(value);
}


extern "C" void Helpers_bytesToGuid(uint8_t *data, GUID *iid) {
    *iid = *reinterpret_cast<const GUID *>(data);
}

extern "C" PCWSTR Helpers_WindowsGetStringRawBuffer(HSTRING string,
                                                    UINT32 *length) {
    return WindowsGetStringRawBuffer(string, length);
}

extern "C" BOOL Helpers_IsTdInterface(DWORD value) {
    return IsTdInterface(value);
}

extern "C" BOOL Helpers_IsTdClass(DWORD value) {
    return IsTdClass(value);
}

extern "C" BOOL Helpers_IsEvSpecialName(DWORD value) {
    return IsEvSpecialName(value);
}

extern "C" void Helpers_generate_id_name(PCWSTR* namePartsW, wchar_t* declarationFullName, DWORD * namePartsCount) {
    using namespace Microsoft::WRL::Wrappers;
    HSTRING* nameParts{ nullptr };
    RoParseTypeName(HStringReference(declarationFullName).Get(), namePartsCount, &nameParts);
    
    size_t count = (size_t)(namePartsCount);
    for (size_t i = 0; i < count; ++i) {
        namePartsW[i] = WindowsGetStringRawBuffer(nameParts[i], nullptr);
    }


}

extern "C" void Helpers_toString_length(PCWSTR value, size_t * count) {
    using namespace Microsoft::WRL::Wrappers;
    *count = wcslen(value);
}


extern "C" BOOL Helpers_IsTdSealed(DWORD value) {
    return IsTdSealed(value);
}


/*
extern "C" HRESULT GenericInstanceIdBuilder_locatorImpl(PCWSTR name, iro_simple_meta_data_builder & builder) {
    shared_ptr<Declaration> declaration{ metadataReader.findByName(name) };
    ASSERT(declaration);

    DeclarationKind kind{ declaration->kind() };
    switch (kind) {
    case DeclarationKind::Class: {
        ClassDeclaration* classDeclaration{ static_cast<ClassDeclaration*>(declaration.get()) };
        const InterfaceDeclaration& defaultInterface{ classDeclaration->defaultInterface() };
        IID defaultInterfaceId = defaultInterface.id();
        ASSERT_SUCCESS(builder.set_runtime_class_simple_default(name, defaultInterface.fullName().data(), &defaultInterfaceId));
        return S_OK;
    }

    case DeclarationKind::Interface: {
        InterfaceDeclaration* interfaceDeclaration{ static_cast<InterfaceDeclaration*>(declaration.get()) };
        ASSERT_SUCCESS(builder.set_win_rt_interface(interfaceDeclaration->id()));
        return S_OK;
    }

    case DeclarationKind::GenericInterface: {
        GenericInterfaceDeclaration* genericInterfaceDeclaration{ static_cast<GenericInterfaceDeclaration*>(declaration.get()) };
        ASSERT_SUCCESS(builder.set_parameterized_interface(genericInterfaceDeclaration->id(), genericInterfaceDeclaration->number_of_generic_parameters()));
        return S_OK;
    }

    case DeclarationKind::Enum: {
        EnumDeclaration* enumDeclaration{ static_cast<EnumDeclaration*>(declaration.get()) };
        ASSERT_SUCCESS(builder.set_enum(enumDeclaration->fullName().data(), Signature::toString(nullptr, enumDeclaration->type()).data()));
        return S_OK;
    }

    case DeclarationKind::Struct: {
        StructDeclaration* structDeclaration{ static_cast<StructDeclaration*>(declaration.get()) };

        vector<wstring> fieldNames;
        for (const StructFieldDeclaration& field : *structDeclaration) {
            fieldNames.push_back(Signature::toString(field._metadata.Get(), field.type()));
        }

        vector<wchar_t*> fieldNamesW;
        for (wstring& fieldName : fieldNames) {
            fieldNamesW.push_back(const_cast<wchar_t*>(fieldName.data()));
        }

        ASSERT_SUCCESS(builder.set_struct(structDeclaration->fullName().data(), structDeclaration->size(), fieldNamesW.data()));
        return S_OK;
    }

    case DeclarationKind::Delegate: {
        DelegateDeclaration* delegateDeclaration{ static_cast<DelegateDeclaration*>(declaration.get()) };
        ASSERT_SUCCESS(builder.set_delegate(delegateDeclaration->id()));
        return S_OK;
    }

    case DeclarationKind::GenericDelegate: {
        GenericDelegateDeclaration* genericDelegateDeclaration{ static_cast<GenericDelegateDeclaration*>(declaration.get()) };
        ASSERT_SUCCESS(builder.set_parameterized_delegate(genericDelegateDeclaration->id(), genericDelegateDeclaration->number_of_generic_parameters()));
        return S_OK;
    }

    default:
        ASSERT_NOT_REACHED();
    }
}


extern "C" void GenericInstanceIdBuilder_generateId(wchar_t * fullName, GUID * iid) {
    wstring declarationFullName = std::wstring(fullName);

    HSTRING* nameParts{ nullptr };
    DWORD namePartsCount{ 0 };
    ASSERT_SUCCESS(RoParseTypeName(HStringReference(declarationFullName.data()).Get(), &namePartsCount, &nameParts));

    array<const wchar_t*, 128> namePartsW;
    ASSERT(namePartsCount < namePartsW.size());

    for (size_t i = 0; i < namePartsCount; ++i) {
        namePartsW[i] = WindowsGetStringRawBuffer(nameParts[i], nullptr);
    }


    ASSERT_SUCCESS(ro_get_parameterized_type_instance_iid(namePartsCount, namePartsW.data(), Ro::Locator(&GenericInstanceIdBuilder_locatorImpl), &guid, nullptr));

    for (size_t i = 0; i < namePartsCount; ++i) {
        ASSERT_SUCCESS(WindowsDeleteString(nameParts[i]));
    }

    CoTaskMemFree(nameParts);

    return guid;
}

*/

extern "C" HRESULT Helpers_RO_E_METADATA_NAME_IS_NAMESPACE = RO_E_METADATA_NAME_IS_NAMESPACE;

extern "C" mdToken Helpers_mdTokenNil = mdTokenNil;
extern "C" mdToken Helpers_mdModuleNil = mdModuleNil;
extern "C" mdToken Helpers_mdTypeRefNil = mdTypeRefNil;
extern "C" mdToken Helpers_mdTypeDefNil = mdTypeDefNil;
extern "C" mdToken Helpers_mdFieldDefNil = mdFieldDefNil;
extern "C" mdToken Helpers_mdMethodDefNil = mdMethodDefNil;
extern "C" mdToken Helpers_mdParamDefNil = mdParamDefNil;
extern "C" mdToken Helpers_mdInterfaceImplNil = mdInterfaceImplNil;
extern "C" mdToken Helpers_mdMemberRefNil = mdMemberRefNil;
extern "C" mdToken Helpers_mdCustomAttributeNil = mdCustomAttributeNil;
extern "C" mdToken Helpers_mdPermissionNil = mdPermissionNil;
extern "C" mdToken Helpers_mdSignatureNil = mdSignatureNil;
extern "C" mdToken Helpers_mdEventNil = mdEventNil;
extern "C" mdToken Helpers_mdPropertyNil = mdPropertyNil;
extern "C" mdToken Helpers_mdModuleRefNil = mdModuleRefNil;
extern "C" mdToken Helpers_mdTypeSpecNil = mdTypeSpecNil;
extern "C" mdToken Helpers_mdAssemblyNil = mdAssemblyNil;
extern "C" mdToken Helpers_mdAssemblyRefNil = mdAssemblyRefNil;
extern "C" mdToken Helpers_mdFileNil = mdFileNil;
extern "C" mdToken Helpers_mdExportedTypeNil = mdExportedTypeNil;
extern "C" mdToken Helpers_mdManifestResourceNil = mdManifestResourceNil;

extern "C" mdToken Helpers_mdGenericParamNil = mdGenericParamNil;
extern "C" mdToken Helpers_mdGenericParamConstraintNil = mdGenericParamConstraintNil;
extern "C" mdToken Helpers_mdMethodSpecNil = mdMethodSpecNil;

extern "C" mdToken Helpers_mdStringNil = mdStringNil;