#pragma once


#ifdef NATIVESCRIPT_EXPORTS
#define NATIVESCRIPT_API __declspec(dllexport)
#else 
#define NATIVESCRIPT_API __declspec(dllimport)
#endif

extern "C" NATIVESCRIPT_API void start(
	const char * BasePath,
const char * ApplicationPath,
bool IsDebug,
bool LogToSystemConsole
);