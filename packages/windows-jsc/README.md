# @nativescript/windows-jsc

NativeScript Windows runtime on **JavaScriptCore**.


## The engine binary — Playwright's WebKit (current)
The official WebKit WinCairo buildbot is dead (last Windows build Sept 2024), and there's no NuGet/npm
prebuilt. The **current** source of a Windows `JavaScriptCore.dll` is **Playwright**, which rebuilds
WebKit constantly:

```
https://playwright.download.prss.microsoft.com/dbazure/download/playwright/builds/webkit/<rev>/webkit-win64.zip
```