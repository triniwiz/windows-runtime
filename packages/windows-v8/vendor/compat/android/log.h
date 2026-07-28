// [windows port] stub for napi-android's v8-api.cpp, which does:
//   #include <android/log.h>
//   #define printf(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)
// On Windows we just no-op the log sink (its debug printfs vanish).
#pragma once
#define ANDROID_LOG_INFO 4
static inline int __android_log_print(int, const char*, ...) { return 0; }
