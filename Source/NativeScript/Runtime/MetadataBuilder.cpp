#include "pch.h"
#include "MetadataBuilder.h"
#include "Helpers.h"
#include "Metadata/MetadataReader.h"
#include "Metadata/Declaration.h"

using namespace v8;

namespace org {
	namespace nativescript {
        void MetadataBuilder::RegisterConstantsOnGlobalObject(Isolate* isolate, Local<ObjectTemplate> globalTemplate, bool isWorkerThread) {
            GlobalHandlerContext* handlerContext = new GlobalHandlerContext(isWorkerThread);
            Local<External> ext = External::New(isolate, handlerContext);

            NamedPropertyHandlerConfiguration config(MetadataBuilder::GlobalPropertyGetter, nullptr, nullptr, nullptr, nullptr, ext, PropertyHandlerFlags::kNonMasking);
            globalTemplate->SetHandler(config);
        }


        void MetadataBuilder::GlobalPropertyGetter(Local<v8::Name> property, const PropertyCallbackInfo<Value>& info) {
            Isolate* isolate = info.GetIsolate();
            std::string propName = ToString(isolate, property.As<v8::String>());

            GlobalHandlerContext* ctx = static_cast<GlobalHandlerContext*>(info.Data().As<External>()->Value());

            Log("GlobalPropertyGetter: prop " + propName);

           auto object = MetadataReader::findByName(propName);

           if (object == nullptr) {
               return;
           }

           Local<Context> context = isolate->GetCurrentContext();

           if (object->kind() == DeclarationKind::Namespace) {
               Local<v8::External> ext = v8::External::New(isolate, dynamic_cast<void*>(object.get()));
               auto nsTemplate = v8::ObjectTemplate::New(isolate);
               nsTemplate->SetInternalFieldCount(1);
               auto ns = nsTemplate->NewInstance(context).ToLocalChecked();
               ns->SetInternalField(0, ext);
               info.GetReturnValue().Set(ns);
           }
 
        }
	}
}