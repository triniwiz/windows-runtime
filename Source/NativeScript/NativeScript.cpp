#include "pch.h"
#include "NativeScript.h"
#include "Runtime/Runtime.h"
#include "Runtime/RuntimeConfig.h"
#include "Runtime/Helpers.h"

using namespace org::nativescript;
using namespace v8;

std::shared_ptr<Runtime> runtime_;
void NativeScript::start(Config* config)
{
	bool isDebug = false;

#ifdef DEBUG
	isDebug = true;
#endif // DEBUG
	RuntimeConfig.ApplicationPath = config->ApplicationPath;
	RuntimeConfig.BasePath = config->BasePath;
	RuntimeConfig.LogToSystemConsole = config->LogToSystemConsole;
	RuntimeConfig.IsDebug = isDebug;
	runtime_ = std::make_shared<Runtime>();


	std::chrono::high_resolution_clock::time_point t1 = std::chrono::high_resolution_clock::now();
	Isolate* isolate = runtime_->CreateIsolate();
	runtime_->Init(isolate);
	std::chrono::high_resolution_clock::time_point t2 = std::chrono::high_resolution_clock::now();
	double duration = std::chrono::duration_cast<std::chrono::milliseconds>(t2 - t1).count();
	Log("Runtime initialization took " + std::to_string(duration) + std::string(" ms"));

	runtime_->RunMainScript();

}
