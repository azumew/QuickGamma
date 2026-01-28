#pragma once
#include <windows.h>
#include <string>

HRESULT CreateShortcut(const std::wstring& arguments, const std::wstring& description);
