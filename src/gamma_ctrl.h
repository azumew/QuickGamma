#pragma once
#include <windows.h>
#include <vector>
#include <string>

struct MonitorInfo {
    std::wstring deviceName;
    HDC hDC;
    float currentGamma;
    WORD originalRamp[3][256];
};

class GammaController {
public:
    GammaController();
    ~GammaController();

    void RefreshMonitors();
    size_t GetMonitorCount() const;
    const MonitorInfo& GetMonitor(size_t index) const;
    
    bool SetGamma(size_t index, float gamma);
    void SetAllGamma(float gamma);

private:
    std::vector<MonitorInfo> m_monitors;
    void ClearMonitors();
};
