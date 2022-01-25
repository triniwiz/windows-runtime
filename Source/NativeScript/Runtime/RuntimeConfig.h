#pragma once
#include <string>

struct RuntimeConfig {
	std::string BasePath;
	std::string ApplicationPath;
	bool IsDebug;
	bool LogToSystemConsole;
};

extern struct RuntimeConfig RuntimeConfig;