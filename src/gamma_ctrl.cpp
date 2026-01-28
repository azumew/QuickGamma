#include "gamma_ctrl.h"
#include <cmath>
#include <algorithm>

GammaController::GammaController() {
    RefreshMonitors();
}

GammaController::~GammaController() {
    ClearMonitors();
}

void GammaController::ClearMonitors() {
    for (auto& mon : m_monitors) {
        if (mon.hDC) {
            DeleteDC(mon.hDC);
        }
    }
    m_monitors.clear();
}

BOOL CALLBACK MonitorEnumProc(HMONITOR hMonitor, HDC hdcMonitor, LPRECT lprcMonitor, LPARAM dwData) {
    std::vector<MonitorInfo>* pMonitors = (std::vector<MonitorInfo>*)dwData;
    
    MONITORINFOEXW mi;
    mi.cbSize = sizeof(MONITORINFOEXW);
    if (GetMonitorInfoW(hMonitor, &mi)) {
        MonitorInfo info;
        info.deviceName = mi.szDevice;
        info.hDC = CreateDCW(NULL, mi.szDevice, NULL, NULL);
        info.currentGamma = 1.0f;
        
        if (info.hDC) {
            // Get current ramp (or default if fails, but usually we just overwrite)
            // But good to save original if we wanted to restore perfect state.
            // For this app, we assume we control it.
            if (!GetDeviceGammaRamp(info.hDC, info.originalRamp)) {
                // Initialize linear ramp if fail
                for (int i = 0; i < 256; i++) {
                    WORD val = (WORD)((i * 65535) / 255);
                    info.originalRamp[0][i] = val;
                    info.originalRamp[1][i] = val;
                    info.originalRamp[2][i] = val;
                }
            }
            pMonitors->push_back(info);
        }
    }
    return TRUE;
}

#include <map>

void GammaController::RefreshMonitors() {
    // Save current gammas
    std::map<std::wstring, float> savedGammas;
    for (const auto& mon : m_monitors) {
        savedGammas[mon.deviceName] = mon.currentGamma;
    }

    ClearMonitors();
    EnumDisplayMonitors(NULL, NULL, MonitorEnumProc, (LPARAM)&m_monitors);

    // Restore gammas
    for (auto& mon : m_monitors) {
        auto it = savedGammas.find(mon.deviceName);
        if (it != savedGammas.end()) {
            mon.currentGamma = it->second;
            // Re-apply gamma to hardware if needed, or just update state.
            // Since we are "refreshing", maybe hardware state is lost or maybe not.
            // Safer to re-apply if we want to be sure, but might cause flicker.
            // For now, update state so UI shows correct value.
        }
    }
}

size_t GammaController::GetMonitorCount() const {
    return m_monitors.size();
}

const MonitorInfo& GammaController::GetMonitor(size_t index) const {
    return m_monitors[index];
}

bool GammaController::SetGamma(size_t index, float gamma) {
    if (index >= m_monitors.size()) return false;
    
    MonitorInfo& mon = m_monitors[index];
    mon.currentGamma = gamma;
    
    WORD ramp[3][256];
    for (int i = 0; i < 256; i++) {
        // Gamma correction formula: input^(1/gamma)
        // Values are 0-65535
        double input = i / 255.0;
        double corrected = std::pow(input, 1.0 / gamma);
        if (corrected > 1.0) corrected = 1.0;
        if (corrected < 0.0) corrected = 0.0;
        
        WORD val = (WORD)(corrected * 65535);
        ramp[0][i] = val; // Red
        ramp[1][i] = val; // Green
        ramp[2][i] = val; // Blue
    }
    
    return SetDeviceGammaRamp(mon.hDC, ramp);
}

void GammaController::SetAllGamma(float gamma) {
    for (size_t i = 0; i < m_monitors.size(); i++) {
        SetGamma(i, gamma);
    }
}
