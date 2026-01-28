#include <windows.h>
#include <shellapi.h>
#include <commctrl.h>
#include <string>
#include <vector>
#include <map>
#include <sstream>
#include <dwmapi.h>
#include <uxtheme.h>

#include "resource.h"
#include "gamma_ctrl.h"
#include "shortcut.h"

#pragma comment(lib, "dwmapi.lib")
#pragma comment(lib, "uxtheme.lib")

#define WM_TRAYICON (WM_USER + 1)
#define TRAY_ICON_ID 1

// Global Variables
HINSTANCE g_hInst = NULL;
HWND g_hHiddenWnd = NULL;
HWND g_hDlg = NULL;
GammaController g_gammaCtrl;
NOTIFYICONDATA g_nid = {};
std::map<int, float> g_initialGammas; // Store gamma values when dialog opens for Cancel
bool g_isDialogVisible = false;

// Forward declarations
LRESULT CALLBACK WndProc(HWND, UINT, WPARAM, LPARAM);
INT_PTR CALLBACK DialogProc(HWND, UINT, WPARAM, LPARAM);
void ShowMainDialog();
void HideMainDialog(bool apply);
void UpdateSlider(HWND hDlg);
void UpdateLabel(HWND hDlg, int val);
void InitMonitors(HWND hDlg);

// Helper to get selected monitor index
int GetSelectedMonitorIndex(HWND hDlg) {
    return (int)SendDlgItemMessage(hDlg, IDC_COMBO_MONITOR, CB_GETCURSEL, 0, 0);
}

int WINAPI WinMain(HINSTANCE hInstance, HINSTANCE hPrevInstance, LPSTR lpCmdLine, int nCmdShow) {
    g_hInst = hInstance;
    HRESULT hr = CoInitialize(NULL); // For Shortcuts

    // Check CLI arguments
    int argc;
    LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    if (argv && argc > 1) {
        // Advanced parsing: -m [id] -g [val] ... or old -g [val] (for all)
        bool hasAction = false;
        
        // Loop through args
        int currentMon = -1; // -1 = All
        
        for (int i = 1; i < argc; ++i) {
            std::wstring arg = argv[i];
            if ((arg == L"-m" || arg == L"/m") && i + 1 < argc) {
                try {
                    currentMon = std::stoi(argv[++i]);
                } catch(...) {}
            } else if ((arg == L"-g" || arg == L"/g") && i + 1 < argc) {
                try {
                    float g = std::stof(argv[++i]);
                    if (currentMon == -1) {
                        g_gammaCtrl.SetAllGamma(g);
                    } else {
                        // Monitor ID is usually 1-indexed in CLI for user friendliness
                        if (currentMon > 0 && currentMon <= (int)g_gammaCtrl.GetMonitorCount()) {
                            g_gammaCtrl.SetGamma(currentMon - 1, g);
                        }
                    }
                    hasAction = true;
                } catch(...) {}
            }
        }
        
        if (hasAction) {
            LocalFree(argv);
            CoUninitialize();
            return 0;
        }
    }
    LocalFree(argv);

    // Single Instance Check
    HANDLE hMutex = CreateMutexW(NULL, TRUE, L"Local\\QuickGammaInstanceMutex");
    if (GetLastError() == ERROR_ALREADY_EXISTS) {
        // Find existing window and activate it
        HWND hExisting = FindWindowW(L"QuickGammaHidden", NULL);
        if (hExisting) {
            // Send message to show dialog (WM_TRAYICON + WM_LBUTTONUP triggers ShowMainDialog)
            PostMessageW(hExisting, WM_TRAYICON, 0, WM_LBUTTONUP);
        }
        return 0; // Exit this instance
    }

    // GUI Mode Initialization
    InitCommonControls(); // For Slider

    // Register Hidden Window Class
    WNDCLASSEXW wc = { sizeof(WNDCLASSEXW), CS_HREDRAW | CS_VREDRAW, WndProc, 0, 0, 
                      hInstance, LoadIcon(hInstance, MAKEINTRESOURCE(IDI_APP_ICON)), LoadCursor(NULL, IDC_ARROW), 
                      (HBRUSH)(COLOR_WINDOW+1), NULL, L"QuickGammaHidden", NULL };
    RegisterClassExW(&wc);

    // Create Hidden Window
    g_hHiddenWnd = CreateWindowW(L"QuickGammaHidden", L"QuickGamma", WS_OVERLAPPEDWINDOW,
                                CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT,
                                NULL, NULL, hInstance, NULL);

    if (!g_hHiddenWnd) return 1;

    // Create Tray Icon
    g_nid.cbSize = sizeof(NOTIFYICONDATA);
    g_nid.hWnd = g_hHiddenWnd;
    g_nid.uID = TRAY_ICON_ID;
    g_nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    g_nid.uCallbackMessage = WM_TRAYICON;
    g_nid.hIcon = LoadIcon(hInstance, MAKEINTRESOURCE(IDI_APP_ICON)); // Use custom icon
    wcscpy_s(g_nid.szTip, L"QuickGamma");
    Shell_NotifyIcon(NIM_ADD, &g_nid);
    
    // Create Main Dialog (Modeless)
    g_hDlg = CreateDialogW(hInstance, MAKEINTRESOURCEW(IDD_MAIN_DIALOG), NULL, DialogProc);
    
    // Show Main Dialog on Startup
    ShowMainDialog();

    // Start Message Loop
    MSG msg;
    while (GetMessage(&msg, NULL, 0, 0)) {
        if (!g_hDlg || !IsDialogMessage(g_hDlg, &msg)) {
            TranslateMessage(&msg);
            DispatchMessage(&msg);
        }
    }

    Shell_NotifyIcon(NIM_DELETE, &g_nid);
    
    // Reset Gamma on Exit
    g_gammaCtrl.SetAllGamma(1.0f);
    
    if (hMutex) {
        ReleaseMutex(hMutex);
        CloseHandle(hMutex);
    }
    
    CoUninitialize();
    return (int)msg.wParam;
}

// Menu IDs
#define IDM_OPEN 2001
#define IDM_QUIT 2002

void ShowTrayMenu(HWND hWnd) {
    POINT pt;
    GetCursorPos(&pt);
    HMENU hMenu = CreatePopupMenu();
    AppendMenuW(hMenu, MF_STRING, IDM_OPEN, L"Open");
    AppendMenuW(hMenu, MF_STRING, IDM_QUIT, L"Quit");
    SetForegroundWindow(hWnd); // Required for menu to disappear on outside click
    TrackPopupMenu(hMenu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hWnd, NULL);
    DestroyMenu(hMenu);
}

LRESULT CALLBACK WndProc(HWND hWnd, UINT message, WPARAM wParam, LPARAM lParam) {
    switch (message) {
    case WM_TRAYICON:
        if (lParam == WM_LBUTTONUP) {
            ShowMainDialog();
        } else if (lParam == WM_RBUTTONUP) {
            ShowTrayMenu(hWnd);
        }
        break;
    case WM_COMMAND:
        switch(LOWORD(wParam)) {
            case IDM_OPEN:
                ShowMainDialog();
                break;
            case IDM_QUIT:
                DestroyWindow(hWnd); // Will trigger WM_DESTROY
                break;
        }
        break;
    case WM_DESTROY:
        PostQuitMessage(0);
        break;
    default:
        return DefWindowProc(hWnd, message, wParam, lParam);
    }
    return 0;
}

void ShowMainDialog() {
    if (!g_hDlg) return;
    
    // Refresh monitors
    g_gammaCtrl.RefreshMonitors();
    InitMonitors(g_hDlg);

    // Initial state logic
    g_initialGammas.clear();
    for (size_t i = 0; i < g_gammaCtrl.GetMonitorCount(); i++) {
        g_initialGammas[(int)i] = g_gammaCtrl.GetMonitor(i).currentGamma;
    }

    ShowWindow(g_hDlg, SW_SHOW);
    SetForegroundWindow(g_hDlg);
    g_isDialogVisible = true;
}

void HideMainDialog(bool apply) {
    ShowWindow(g_hDlg, SW_HIDE);
    g_isDialogVisible = false;
    
    if (!apply) {
        for (auto const& [idx, val] : g_initialGammas) {
            g_gammaCtrl.SetGamma(idx, val);
        }
    }
}

// Flag to prevent recursion
bool g_isUpdating = false;

void UpdateLabel(HWND hDlg, int val) {
    if (g_isUpdating) return;
    g_isUpdating = true;
    
    float g = val / 100.0f;
    WCHAR buf[32];
    swprintf_s(buf, L"%.2f", g);
    SetDlgItemTextW(hDlg, IDC_LABEL_VALUE, buf);
    
    g_isUpdating = false;
}

void UpdateSlider(HWND hDlg, float val) {
    if (g_isUpdating) return;
    g_isUpdating = true;
    
    SendDlgItemMessage(hDlg, IDC_SLIDER_GAMMA, TBM_SETPOS, TRUE, (int)(val * 100));
    
    g_isUpdating = false;
}

// DWM Constants for Mica
#ifndef DWMWA_USE_IMMERSIVE_DARK_MODE
#define DWMWA_USE_IMMERSIVE_DARK_MODE 20
#endif

#ifndef DWMWA_SYSTEMBACKDROP_TYPE
#define DWMWA_SYSTEMBACKDROP_TYPE 38
#endif

#ifndef DWMSBT_MAINWINDOW
#define DWMSBT_MAINWINDOW 2
#endif

void EnableMica(HWND hWnd) {
    BOOL dark = FALSE; // Force Light Mode
    DwmSetWindowAttribute(hWnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark, sizeof(dark));

    int backdrop = DWMSBT_MAINWINDOW; // Mica
    DwmSetWindowAttribute(hWnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop, sizeof(backdrop));
}

void SetModernFont(HWND hDlg) {
    HFONT hFont = CreateFontW(14, 0, 0, 0, FW_NORMAL, FALSE, FALSE, FALSE, DEFAULT_CHARSET, 
                             OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, DEFAULT_QUALITY, 
                             DEFAULT_PITCH | FF_DONTCARE, L"Segoe UI");
    SendMessage(hDlg, WM_SETFONT, (WPARAM)hFont, TRUE);
    EnumChildWindows(hDlg, [](HWND hWnd, LPARAM lParam) -> BOOL {
        SendMessage(hWnd, WM_SETFONT, (WPARAM)lParam, TRUE);
        return TRUE;
    }, (LPARAM)hFont);
}

void EnableDarkModeControls(HWND hDlg) {
    // Attempt to use system "Explorer" theme for controls
    EnumChildWindows(hDlg, [](HWND hWnd, LPARAM lParam) -> BOOL {
        WCHAR className[256];
        GetClassNameW(hWnd, className, 256);
        
        if (wcscmp(className, L"Button") == 0 || 
            wcscmp(className, L"msctls_trackbar32") == 0 ||
            wcscmp(className, L"ComboBox") == 0 ||
            wcscmp(className, L"Edit") == 0) {
             SetWindowTheme(hWnd, L"Explorer", NULL); 
        }
        return TRUE;
    }, 0);
}

void InitMonitors(HWND hDlg) {

    // Save selection
    int curSel = (int)SendDlgItemMessage(hDlg, IDC_COMBO_MONITOR, CB_GETCURSEL, 0, 0);
    
    SendDlgItemMessage(hDlg, IDC_COMBO_MONITOR, CB_RESETCONTENT, 0, 0);
    size_t count = g_gammaCtrl.GetMonitorCount();
    for (size_t i = 0; i < count; i++) {
        const auto& mon = g_gammaCtrl.GetMonitor(i);
        std::wstring name = L"Monitor " + std::to_wstring(i+1);
        SendDlgItemMessageW(hDlg, IDC_COMBO_MONITOR, CB_ADDSTRING, 0, (LPARAM)name.c_str());
    }
    
    if (count > 0) {
        if (curSel < 0 || curSel >= count) curSel = 0;
        SendDlgItemMessage(hDlg, IDC_COMBO_MONITOR, CB_SETCURSEL, curSel, 0);
        
        float g = g_gammaCtrl.GetMonitor(curSel).currentGamma;
        SendDlgItemMessage(hDlg, IDC_SLIDER_GAMMA, TBM_SETRANGE, TRUE, MAKELONG(20, 400));
        SendDlgItemMessage(hDlg, IDC_SLIDER_GAMMA, TBM_SETPOS, TRUE, (int)(g * 100));
        
        WCHAR buf[32];
        swprintf_s(buf, L"%.2f", g);
        SetDlgItemTextW(hDlg, IDC_LABEL_VALUE, buf);
    }
}

INT_PTR CALLBACK DialogProc(HWND hDlg, UINT message, WPARAM wParam, LPARAM lParam) {
    // Light grey-ish modern background (Win11 default app background approx)
    static HBRUSH hBrushBg = CreateSolidBrush(RGB(243, 243, 243)); 
    
    switch (message) {
    case WM_INITDIALOG:
        // Disable Mica to ensure solid painting (Optional, but safe)
        // EnableMica(hDlg); // Commented out to prefer solid background
        
        SetModernFont(hDlg);
        EnableDarkModeControls(hDlg); // Keeps "Explorer" theme (visual styles)
        SendDlgItemMessage(hDlg, IDC_SLIDER_GAMMA, TBM_SETRANGE, TRUE, MAKELONG(20, 400));
        return (INT_PTR)TRUE;

    case WM_CTLCOLORDLG:
        return (INT_PTR)hBrushBg;

    case WM_CTLCOLORSTATIC:
        SetBkMode((HDC)wParam, TRANSPARENT);
        // SetBkColor((HDC)wParam, RGB(243, 243, 243)); // Match background
        return (INT_PTR)hBrushBg; 
        // Returning the same solid brush ensures the control background 
        // matches the dialog background.

    case WM_CLOSE:
        HideMainDialog(false);
        return (INT_PTR)TRUE;

    case WM_HSCROLL:
        if ((HWND)lParam == GetDlgItem(hDlg, IDC_SLIDER_GAMMA)) {
            if (g_isUpdating) break;
            
            int pos = (int)SendMessage((HWND)lParam, TBM_GETPOS, 0, 0);
            float g = pos / 100.0f;
            UpdateLabel(hDlg, pos); // Updates Text
            
            int sel = GetSelectedMonitorIndex(hDlg);
            if (sel >= 0) {
                g_gammaCtrl.SetGamma(sel, g);
            }
        }
        return (INT_PTR)TRUE;

    case WM_COMMAND:
        if (LOWORD(wParam) == IDOK) {
            HideMainDialog(true);
            return (INT_PTR)TRUE;
        } else if (LOWORD(wParam) == IDCANCEL) {
            HideMainDialog(false);
            return (INT_PTR)TRUE;
        } else if (LOWORD(wParam) == IDC_BTN_DEFAULT) {
            // "Default All" logic
            g_gammaCtrl.SetAllGamma(1.0f);
            
            // Update UI
            SendDlgItemMessage(hDlg, IDC_SLIDER_GAMMA, TBM_SETPOS, TRUE, 100);
            WCHAR buf[32];
            swprintf_s(buf, L"1.00");
            SetDlgItemTextW(hDlg, IDC_LABEL_VALUE, buf);
            
        } else if (LOWORD(wParam) == IDC_BTN_SHORTCUT) {
            // New Shortcut Logic: Save ALL monitor settings
            std::wstringstream args;
            std::wstringstream nameSuffix;
            size_t count = g_gammaCtrl.GetMonitorCount();
            
            for(size_t i=0; i<count; ++i) {
                float g = g_gammaCtrl.GetMonitor(i).currentGamma;
                args << L"-m " << (i + 1) << L" -g " << g << L" ";
                nameSuffix << L"m" << (i + 1) << L"-" << g << L"_";
            }
            
            std::wstring desc = nameSuffix.str();
            if (!desc.empty()) desc.pop_back(); // Remove trailing _
            
            if (SUCCEEDED(CreateShortcut(args.str(), desc))) {
                 MessageBoxW(hDlg, L"Shortcut created.", L"Success", MB_OK);
            } else {
                 MessageBoxW(hDlg, L"Failed to create shortcut.", L"Error", MB_ICONERROR);
            }

        } else if (HIWORD(wParam) == CBN_SELCHANGE && LOWORD(wParam) == IDC_COMBO_MONITOR) {
            int sel = GetSelectedMonitorIndex(hDlg);
            if (sel >= 0) {
                float g = g_gammaCtrl.GetMonitor(sel).currentGamma;
                g_isUpdating = true; // Prevent text update triggering change
                SendDlgItemMessage(hDlg, IDC_SLIDER_GAMMA, TBM_SETPOS, TRUE, (int)(g * 100));
                
                WCHAR buf[32];
                swprintf_s(buf, L"%.2f", g);
                SetDlgItemTextW(hDlg, IDC_LABEL_VALUE, buf);
                g_isUpdating = false;
            }
        } else if (HIWORD(wParam) == EN_CHANGE && LOWORD(wParam) == IDC_LABEL_VALUE) {
            if (!g_isUpdating) {
                WCHAR buf[32];
                GetDlgItemTextW(hDlg, IDC_LABEL_VALUE, buf, 32);
                try {
                     float g = std::stof(buf);
                     if (g >= 0.2f && g <= 4.0f) {
                         UpdateSlider(hDlg, g);
                         int sel = GetSelectedMonitorIndex(hDlg);
                         if (sel >= 0) {
                             g_gammaCtrl.SetGamma(sel, g);
                         }
                     }
                } catch (...) {}
            }
        }
        break;
    }
    return (INT_PTR)FALSE;
}
