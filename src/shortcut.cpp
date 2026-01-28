#include "shortcut.h"
#include <shlobj.h>
#include <shlwapi.h>

HRESULT CreateShortcut(const std::wstring& arguments, const std::wstring& description) {
    HRESULT hres;
    IShellLink* psl;
    
    // Get a pointer to the IShellLink interface.
    hres = CoCreateInstance(CLSID_ShellLink, NULL, CLSCTX_INPROC_SERVER, IID_IShellLink, (LPVOID*)&psl);
    if (SUCCEEDED(hres)) {
        IPersistFile* ppf;
        
        // Get path to current executable
        WCHAR szExePath[MAX_PATH];
        GetModuleFileNameW(NULL, szExePath, MAX_PATH);
        
        psl->SetPath(szExePath);
        psl->SetArguments(arguments.c_str());
        psl->SetDescription(L"QuickGamma Shortcut");
        
        // Set working directory
        WCHAR szDir[MAX_PATH];
        wcscpy_s(szDir, szExePath);
        PathRemoveFileSpecW(szDir);
        psl->SetWorkingDirectory(szDir);

        // Query IShellLink for the IPersistFile interface.
        hres = psl->QueryInterface(IID_IPersistFile, (LPVOID*)&ppf);
        
        if (SUCCEEDED(hres)) {
            WCHAR szPath[MAX_PATH];
            SHGetFolderPathW(NULL, CSIDL_DESKTOPDIRECTORY, NULL, 0, szPath);
            
            std::wstring filename = std::wstring(szPath) + L"\\QuickGamma_" + description + L".lnk";
            
            // Save the link by calling IPersistFile::Save.
            hres = ppf->Save(filename.c_str(), TRUE);
            ppf->Release();
        }
        psl->Release();
    }
    return hres;
}
