#include "pch.h"
#include "ModuleInternal.h"
#include "NativeScriptException.h"
#include "RuntimeConfig.h"
#include "Caches.h"
#include "Helpers.h"
#include <PathCch.h>
#include <fstream>
#include <iostream>
#include "json/json.h"

using namespace v8;

namespace org {
	namespace nativescript {
        ModuleInternal::ModuleInternal(Local<Context> context) {
            std::string requireFactoryScript =
                "(function() { "
                "    function require_factory(requireInternal, dirName) { "
                "        return function require(modulePath) { "
                "            return requireInternal(modulePath, dirName); "
                "        } "
                "    } "
                "    return require_factory; "
                "})()";

            Isolate* isolate = context->GetIsolate();
            Local<v8::Object> global = context->Global();
            Local<v8::Script> script;
            TryCatch tc(isolate);
            if (!Script::Compile(context, ToV8String(isolate, requireFactoryScript.c_str())).ToLocal(&script) && tc.HasCaught()) {
                LogError(isolate, tc);
                Assert(false, isolate);
            }
            Assert(!script.IsEmpty(), isolate);

            Local<Value> result;
            if (!script->Run(context).ToLocal(&result) && tc.HasCaught()) {
                LogError(isolate, tc);
                Assert(false, isolate);
            }
            Assert(!result.IsEmpty() && result->IsFunction(), isolate);

            this->requireFactoryFunction_ = std::make_unique<Persistent<v8::Function>>(isolate, result.As<v8::Function>());

            Local<FunctionTemplate> requireFuncTemplate = FunctionTemplate::New(isolate, RequireCallback, External::New(isolate, this));
            this->requireFunction_ = std::make_unique<Persistent<v8::Function>>(isolate, requireFuncTemplate->GetFunction(context).ToLocalChecked());

            Local<v8::Function> globalRequire = GetRequireFunction(isolate, RuntimeConfig.ApplicationPath);
            bool success = global->Set(context, ToV8String(isolate, "require"), globalRequire).FromMaybe(false);
            Assert(success, isolate);
        }

        bool ModuleInternal::RunModule(Isolate* isolate, std::string path) {
            std::shared_ptr<Caches> cache = Caches::Get(isolate);
            Local<Context> context = cache->GetContext();
            Local<v8::Object> globalObject = context->Global();
            Local<Value> requireObj;
            bool success = globalObject->Get(context, ToV8String(isolate, "require")).ToLocal(&requireObj);
            Assert(success && requireObj->IsFunction(), isolate);
            Local<v8::Function> requireFunc = requireObj.As<v8::Function>();
            Local<Value> args[] = { ToV8String(isolate, path) };
            Local<Value> result;
            success = requireFunc->Call(context, globalObject, 1, args).ToLocal(&result);
            return success;
        }

        Local<v8::Function> ModuleInternal::GetRequireFunction(Isolate* isolate, const std::string& dirName) {
            Local<v8::Function> requireFuncFactory = requireFactoryFunction_->Get(isolate);
            Local<Context> context = isolate->GetCurrentContext();
            Local<v8::Function> requireInternalFunc = this->requireFunction_->Get(isolate);
            Local<Value> args[2]{
                requireInternalFunc, ToV8String(isolate, dirName.c_str())
            };

            Local<Value> result;
            Local<v8::Object> thiz = Object::New(isolate);
            bool success = requireFuncFactory->Call(context, thiz, 2, args).ToLocal(&result);
            Assert(success && !result.IsEmpty() && result->IsFunction(), isolate);

            return result.As<v8::Function>();
        }

        void ModuleInternal::RequireCallback(const FunctionCallbackInfo<Value>& info) {
            Isolate* isolate = info.GetIsolate();

            try {
                ModuleInternal* moduleInternal = static_cast<ModuleInternal*>(info.Data().As<External>()->Value());

                std::string moduleName = ToString(isolate, info[0].As<v8::String>());
                std::string callingModuleDirName = ToString(isolate, info[1].As<v8::String>());

                std::string fullPath;
                if (moduleName.length() > 0 && moduleName[0] != '/') {
                    if (moduleName[0] == '.') {
                        fullPath = JoinPath(callingModuleDirName, moduleName);
                    }
                    else if (moduleName[0] == '~') {
                        moduleName = moduleName.substr(2);
                        fullPath = JoinPath(RuntimeConfig.ApplicationPath.c_str(),moduleName.c_str());
                    }
                    else {
                        std::string tnsModulesPath = JoinPath(RuntimeConfig.ApplicationPath.c_str(),"tns_modules");
                        fullPath = JoinPath(tnsModulesPath, moduleName.c_str());

                        const char* path1 = fullPath.c_str();
                        std::string path2_tmp = JoinPath(fullPath, "js");
                        const char* path2 = path2_tmp.c_str();

                        if (!Exists(path1) && !Exists(path2)) {
                            fullPath = JoinPath(tnsModulesPath,"tns-core-modules");
                            fullPath = JoinPath(fullPath, moduleName.c_str());
                        }
                    }
                }
                else {
                    fullPath = std::string(moduleName.c_str());
                }

                std::string fileNameOnly = GetFileName(fullPath);
                size_t len = strlen(fullPath.c_str()) - strlen(fileNameOnly.c_str());
                std::string pathOnly = fullPath.substr(0, len);


                bool isData = false;
                Local<v8::Object> moduleObj = moduleInternal->LoadImpl(isolate, fileNameOnly.c_str(), pathOnly.c_str(), isData);
                if (moduleObj.IsEmpty()) {
                    return;
                }

                if (isData) {
                    Assert(!moduleObj.IsEmpty(), isolate);
                    info.GetReturnValue().Set(moduleObj);
                }
                else {
                    Local<Context> context = isolate->GetCurrentContext();
                    Local<Value> exportsObj;
                    bool success = moduleObj->Get(context, ToV8String(isolate, "exports")).ToLocal(&exportsObj);
                    Assert(success, isolate);
                    info.GetReturnValue().Set(exportsObj);
                }
            }
            catch (NativeScriptException& ex) {
                ex.ReThrowToV8(isolate);
            }
        }

        Local<v8::Object> ModuleInternal::LoadImpl(Isolate* isolate, const std::string& moduleName, const std::string& baseDir, bool& isData) {
            size_t lastIndex = moduleName.find_last_of(".");
            std::string moduleNameWithoutExtension = (lastIndex == std::string::npos) ? moduleName : moduleName.substr(0, lastIndex);
            std::string cacheKey = baseDir + "*" + moduleNameWithoutExtension;
            auto it = this->loadedModules_.find(cacheKey);

            if (it != this->loadedModules_.end()) {
                return it->second->Get(isolate);
            }

            Local<v8::Object> moduleObj;
            Local<v8::Value> exportsObj;
            std::string path = this->ResolvePath(isolate, baseDir, moduleName);
            if (path.empty()) {
                return Local<v8::Object>();
            }

            std::string pathStr = std::string(path.c_str());
            std::string extension = GetFileExtension(pathStr);
            if (IsStringEqual(extension,"json")) {
                isData = true;
            }

            auto it2 = this->loadedModules_.find(path);
            if (it2 != this->loadedModules_.end()) {
                return it2->second->Get(isolate);
            }

            if (IsStringEqual(extension, "js")) {
                moduleObj = this->LoadModule(isolate, path, cacheKey);
            }
            else if (IsStringEqual(extension, "json")) {
                moduleObj = this->LoadData(isolate, path);
            }
            else {
                // TODO: throw an error for unsupported file extension
                Assert(false, isolate);
            }

            return moduleObj;
        }

        Local<v8::Object> ModuleInternal::LoadModule(Isolate* isolate, const std::string& modulePath, const std::string& cacheKey) {
            Local<v8::Object> moduleObj = Object::New(isolate);
            Local<v8::Object> exportsObj = Object::New(isolate);
            Local<Context> context = isolate->GetCurrentContext();
            bool success = moduleObj->Set(context, ToV8String(isolate, "exports"), exportsObj).FromMaybe(false);
            Assert(success, isolate);

            const PropertyAttribute readOnlyFlags = static_cast<PropertyAttribute>(PropertyAttribute::DontDelete | PropertyAttribute::ReadOnly);

            Local<v8::String> fileName = ToV8String(isolate, modulePath);
            success = moduleObj->DefineOwnProperty(context, ToV8String(isolate, "id"), fileName, readOnlyFlags).FromMaybe(false);
            Assert(success, isolate);

            std::shared_ptr<Persistent<v8::Object>> poModuleObj = std::make_shared<Persistent<v8::Object>>(isolate, moduleObj);
            TempModule tempModule(this, modulePath, cacheKey, poModuleObj);

            Local<v8::Script> script = LoadScript(isolate, modulePath);

            Local<v8::Function> moduleFunc;
            {
                TryCatch tc(isolate);
                moduleFunc = script->Run(context).ToLocalChecked().As<v8::Function>();
                if (tc.HasCaught()) {
                    throw NativeScriptException(isolate, tc, "Error running script " + modulePath);
                }
            }

            std::string parentDir = GetPathByDeletingLastComponent(modulePath.c_str());
            Local<v8::Function> require = GetRequireFunction(isolate, parentDir);
            Local<Value> requireArgs[5]{
                moduleObj, exportsObj, require, ToV8String(isolate, modulePath.c_str()), ToV8String(isolate, parentDir.c_str())
            };

            success = moduleObj->Set(context, ToV8String(isolate, "require"), require).FromMaybe(false);
            Assert(success, isolate);

            {
                TryCatch tc(isolate);
                Local<Value> result;
                Local<v8::Object> thiz = Object::New(isolate);
                success = moduleFunc->Call(context, thiz, sizeof(requireArgs) / sizeof(Local<Value>), requireArgs).ToLocal(&result);
                if (!success || tc.HasCaught()) {
                    throw NativeScriptException(isolate, tc, "Error calling module function");
                }
            }

            tempModule.SaveToCache();
            return moduleObj;
        }

        Local<v8::Object> ModuleInternal::LoadData(Isolate* isolate, const std::string& modulePath) {
            Local<v8::Object> json;

            std::string jsonData = ReadText(modulePath);

            Local<v8::String> jsonStr = ToV8String(isolate, jsonData);

            Local<Context> context = isolate->GetCurrentContext();
            TryCatch tc(isolate);
            MaybeLocal<Value> maybeValue = JSON::Parse(context, jsonStr);
            if (maybeValue.IsEmpty() || tc.HasCaught()) {
                std::string errMsg = "Cannot parse JSON file " + modulePath;
                throw NativeScriptException(isolate, tc, errMsg);
            }

            Local<Value> value = maybeValue.ToLocalChecked();
            if (!value->IsObject()) {
                std::string errMsg = "JSON is not valid, file=" + modulePath;
                throw NativeScriptException(errMsg);
            }

            json = value.As<Object>();

            this->loadedModules_.emplace(modulePath, std::make_shared<Persistent<Object>>(isolate, json));

            return json;
        }

        Local<v8::Script> ModuleInternal::LoadScript(Isolate* isolate, const std::string& path) {
            Local<Context> context = isolate->GetCurrentContext();
            std::string baseOrigin = ReplaceAll(path, RuntimeConfig.BasePath, "");
            std::string fullRequiredModulePathWithSchema = "file://" + baseOrigin;
            ScriptOrigin origin(isolate, ToV8String(isolate, fullRequiredModulePathWithSchema));
            Local<v8::String> scriptText = WrapModuleContent(isolate, path);
            ScriptCompiler::CachedData* cacheData = LoadScriptCache(path);
            ScriptCompiler::Source source(scriptText, origin, cacheData);

            ScriptCompiler::CompileOptions options = ScriptCompiler::kNoCompileOptions;

            if (cacheData != nullptr) {
                options = ScriptCompiler::kConsumeCodeCache;
            }

            Local<v8::Script> script;
            TryCatch tc(isolate);
            bool success = ScriptCompiler::Compile(context, &source, options).ToLocal(&script);
            if (!success || tc.HasCaught()) {
                throw NativeScriptException(isolate, tc, "Cannot compile " + path);
            }

            if (cacheData == nullptr) {
                SaveScriptCache(script, path);
            }

            return script;
        }

        Local<v8::String> ModuleInternal::WrapModuleContent(Isolate* isolate, const std::string& path) {
            return ReadModule(isolate, path);
        }

        std::string ModuleInternal::ResolvePath(Isolate* isolate, const std::string& baseDir, const std::string& moduleName) {
            std::string baseDirStr = std::string(baseDir.c_str());
            std::string moduleNameStr = std::string(moduleName.c_str());
            std::string fullPath = JoinPath(baseDirStr, moduleNameStr);

            bool isDirectory = GetIsDirectory(fullPath.c_str());
            bool exists = Exists(fullPath.c_str());

            if (exists && isDirectory) {
                std::string jsFile = JoinPath(fullPath,"js");
                bool isDir = GetIsDirectory(jsFile.c_str());
                if (Exists(jsFile.c_str()) && !isDir) {
                    return jsFile;
                }
            }

            if (!exists) {
                fullPath = JoinPath(fullPath, "js");
                isDirectory = GetIsDirectory(fullPath.c_str());
                exists = Exists(fullPath.c_str());
            }

            if (!exists) {
                throw NativeScriptException("The specified module does not exist: " + moduleName);
            }

            if (!isDirectory) {
                return fullPath;
            }

            // Try to resolve module from main entry in package.json
            std::string packageJson = JoinPath(fullPath, "package.json");
            bool error = false;
            std::string entry = this->ResolvePathFromPackageJson(packageJson.c_str(), error);
            if (error) {
                throw NativeScriptException("Unable to locate main entry in " + std::string(packageJson));
            }

            if (!entry.empty()) {
                fullPath = std::string(entry.c_str());
            }

          
            isDirectory = GetIsDirectory(fullPath.c_str());
            exists = Exists(fullPath.c_str());

            if (exists && !isDirectory) {
                return fullPath;
            }

            if (!exists) {
                fullPath = JoinPath(fullPath,"js");
            }
            else {
                fullPath = JoinPath(fullPath, "index.js");
            }

           
            isDirectory = GetIsDirectory(fullPath.c_str());
            exists = Exists(fullPath.c_str());

            if (!exists) {
                throw NativeScriptException("The specified module does not exist: " + moduleName);
            }

            return fullPath;
        }

        std::string ModuleInternal::ResolvePathFromPackageJson(const std::string& packageJson, bool& error) {
            std::string packageJsonStr = std::string(packageJson.c_str());

            bool isDirectory = GetIsDirectory(packageJsonStr.c_str());
            bool exists = Exists(packageJsonStr.c_str());
            if (!exists || isDirectory) {
                return std::string();
            }
            
            auto data = ReadFileInternal(packageJsonStr);

            if (data == nullptr) {
                return std::string();
            }

            std::string dicStr = std::string((char*)data);

 
			Json::Value dic;
			Json::Reader reader;
			bool didRead = reader.parse(dicStr, dic);

            if (!didRead) {
                error = true;
                return std::string();
            }

			std::string main = dic["main"].asString();

            if (main.empty()) {
                return std::string();
            }

             std::string path = JoinPath(GetPathByDeletingLastComponent(packageJsonStr), main);


             exists = Exists(path.c_str());
             isDirectory = GetIsDirectory(path.c_str());

            if (exists && isDirectory) {
                packageJsonStr = JoinPath(path,"package.json");
                exists = Exists(path.c_str());
                isDirectory = GetIsDirectory(path.c_str());

                if (exists && !isDirectory) {
                    return this->ResolvePathFromPackageJson(packageJsonStr.c_str(), error);
                }
            }

            return path;
        }

        ScriptCompiler::CachedData* ModuleInternal::LoadScriptCache(const std::string& path) {
            if (RuntimeConfig.IsDebug) {
                return nullptr;
            }

            long length = 0;
            std::string cachePath = GetCacheFileName(path + ".cache");

            struct stat result;
            if (stat(cachePath.c_str(), &result) == 0) {
                auto cacheLastModifiedTime = result.st_mtime;
                if (stat(path.c_str(), &result) == 0) {
                    auto jsLastModifiedTime = result.st_mtime;
                    if (jsLastModifiedTime > 0 && cacheLastModifiedTime > 0 && jsLastModifiedTime > cacheLastModifiedTime) {
                        // The javascript file is more recent than the cache file => ignore the cache
                        return nullptr;
                    }
                }
            }

            bool isNew = false;
            uint8_t* data = ReadBinary(cachePath, length, isNew);
            if (!data) {
                return nullptr;
            }

            return new ScriptCompiler::CachedData(data, (int)length, isNew ? ScriptCompiler::CachedData::BufferOwned : ScriptCompiler::CachedData::BufferNotOwned);
        }

        void ModuleInternal::SaveScriptCache(const Local<Script> script, const std::string& path) {
            if (RuntimeConfig.IsDebug) {
                return;
            }

            Local<UnboundScript> unboundScript = script->GetUnboundScript();
            ScriptCompiler::CachedData* cachedData = ScriptCompiler::CreateCodeCache(unboundScript);

            int length = cachedData->length;
            std::string cachePath = GetCacheFileName(path + ".cache");
            WriteBinary(cachePath, cachedData->data, length);
        }

        std::string ModuleInternal::GetCacheFileName(const std::string& path) {
            std::string key = path.substr(RuntimeConfig.ApplicationPath.size() + 1);
            std::replace(key.begin(), key.end(), '/', '-');

            TCHAR lpTempPathBuffer[MAX_PATH];
            TCHAR szTempFileName[MAX_PATH];

            if(!GetTempPath(MAX_PATH, lpTempPathBuffer)) {
                // Failed to get temp dir
                Assert(false);
            }

            std::wstring wkey(key.begin(), key.end());

            if(!GetTempFileName(lpTempPathBuffer, wkey.c_str(),0, szTempFileName)){
                // Failed to get temp file
                Assert(false);
            }
            std::wstring name(szTempFileName);

            return std::string(name.begin(), name.end());
        }


       
	}
}