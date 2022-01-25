#pragma once
#include <string>

struct Config {
	std::string BasePath;
	std::string ApplicationPath;
	bool IsDebug;
	bool LogToSystemConsole;
};

class NativeScript {
public:
	static void start(Config* config);
};