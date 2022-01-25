#include "pch.h"
#include "Nativescript.DLL.h"

void start(const char * BasePath,const char * ApplicationPath, bool IsDebug, bool LogToSystemConsole)
{
	Config config;
	config.BasePath = std::string(BasePath);
	config.ApplicationPath = std::string(ApplicationPath);
	config.IsDebug = true;
	config.LogToSystemConsole = LogToSystemConsole;
	NativeScript::start(&config);
}
