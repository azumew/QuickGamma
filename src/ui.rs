use crate::{
    gamma::{displays, token, Display},
    *,
};
use std::{cell::RefCell, collections::BTreeMap, ptr::null_mut};
use windows_sys::Win32::{
    Graphics::Dwm::DwmSetWindowAttribute,
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::GetModuleHandleW},
    UI::{Controls::*, Shell::*},
};
const COMBO: i32 = 1001;
const SLIDER: i32 = 1002;
const EDIT: i32 = 1003;
const TBM_GETPOS: u32 = WM_USER;
const TRAY: u32 = WM_APP + 10;
const MAGIC: usize = 0x514732;
thread_local! {static APP:RefCell<Option<App>>=const{RefCell::new(None)};}
struct App {
    worker: Worker,
    screens: Vec<Display>,
    values: BTreeMap<u64, u16>,
    before: BTreeMap<u64, u16>,
    restart: u32,
    tray_ok: bool,
}
impl App {
    fn val(&self, i: usize) -> u16 {
        self.values
            .get(&token(&self.screens[i]))
            .copied()
            .unwrap_or(100)
    }
    fn set(&mut self, i: usize, v: u16) -> Result<()> {
        let k = token(&self.screens[i]);
        if self.val(i) != v {
            self.worker.set(k, v)?;
            self.values.insert(k, v);
        }
        Ok(())
    }
    fn batch(&mut self, changes: &[(usize, u16)]) -> Result<()> {
        let mut applied = Vec::new();
        for &(i, v) in changes {
            let old = self.val(i);
            if let Err(e) = self.set(i, v) {
                let mut failed = false;
                for (j, prev) in applied.into_iter().rev() {
                    if self.set(j, prev).is_err() {
                        failed = true
                    }
                }
                return if failed {
                    Err(err(&format!("{e}。一部の変更を取り消せませんでした")))
                } else {
                    Err(e)
                };
            }
            applied.push((i, old))
        }
        Ok(())
    }
    unsafe fn selected(&self, h: HWND) -> Option<usize> {
        let i = SendDlgItemMessageW(h, COMBO, CB_GETCURSEL, 0, 0) as usize;
        (i < self.screens.len()).then_some(i)
    }
    unsafe fn sync(&self, h: HWND) {
        if let Some(i) = self.selected(h) {
            let v = self.val(i);
            SendDlgItemMessageW(h, SLIDER, TBM_SETPOS, 1, v as isize);
            SetDlgItemTextW(
                h,
                EDIT,
                wide(&format!("{}.{:02}", v / 100, v % 100)).as_ptr(),
            );
        } else {
            SetDlgItemTextW(h, EDIT, wide("").as_ptr());
        }
    }
    unsafe fn refresh(&mut self, h: HWND) {
        let old = self.selected(h).map(|i| token(&self.screens[i]));
        self.screens = displays();
        SendDlgItemMessageW(h, COMBO, CB_RESETCONTENT, 0, 0);
        let mut sel = 0;
        for (i, d) in self.screens.iter().enumerate() {
            if Some(token(d)) == old {
                sel = i
            }
            let label = wide(&format!("モニター {}", i + 1));
            SendDlgItemMessageW(h, COMBO, CB_ADDSTRING, 0, label.as_ptr() as isize);
        }
        SendDlgItemMessageW(h, COMBO, CB_SETCURSEL, sel, 0);
        self.sync(h);
    }
    unsafe fn edit(&mut self, h: HWND) -> Result<()> {
        if let Some(i) = self.selected(h) {
            let mut b = [0; 64];
            let n = GetDlgItemTextW(h, EDIT, b.as_mut_ptr(), 64);
            self.set(i, value(&String::from_utf16_lossy(&b[..n as usize]))?)?;
        }
        Ok(())
    }
    unsafe fn open(&mut self, h: HWND) {
        if IsWindowVisible(h) == 0 {
            self.before = self.values.clone();
        }
        self.refresh(h);
        ShowWindow(h, SW_RESTORE);
        SetForegroundWindow(h);
    }
    unsafe fn hide(&self, h: HWND) {
        if self.tray_ok {
            ShowWindow(h, SW_HIDE);
        } else {
            ShowWindow(h, SW_MINIMIZE);
        }
    }
    unsafe fn command(&mut self, h: HWND, id: i32, event: u32) -> Result<()> {
        match (id, event) {
            (COMBO, CBN_SELCHANGE) => self.sync(h),
            (EDIT, EN_CHANGE) => {
                if let Some(i) = self.selected(h) {
                    let mut b = [0; 64];
                    let n = GetDlgItemTextW(h, EDIT, b.as_mut_ptr(), 64);
                    if let Ok(v) = value(&String::from_utf16_lossy(&b[..n as usize])) {
                        self.set(i, v)?;
                        SendDlgItemMessageW(h, SLIDER, TBM_SETPOS, 1, v as isize);
                    }
                }
            }
            (1, _) => {
                self.edit(h)?;
                self.hide(h);
            }
            (2, _) => {
                let changes: Vec<_> = self
                    .screens
                    .iter()
                    .enumerate()
                    .map(|(i, d)| (i, self.before.get(&token(d)).copied().unwrap_or(100)))
                    .collect();
                self.batch(&changes)?;
                self.sync(h);
                self.hide(h);
            }
            (1004, _) => {
                self.batch(
                    &(0..self.screens.len())
                        .map(|i| (i, 100))
                        .collect::<Vec<_>>(),
                )?;
                self.sync(h);
            }
            (1005, _) => {
                self.edit(h)?;
                if self.screens.is_empty() {
                    return Err(err("モニターがありません"));
                }
                let args = (0..self.screens.len())
                    .map(|i| {
                        format!(
                            "-m {} -g {}.{:02}",
                            i + 1,
                            self.val(i) / 100,
                            self.val(i) % 100
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                shortcut(&args)?;
                MessageBoxW(
                    h,
                    wide("デスクトップにショートカットを作成しました。").as_ptr(),
                    wide("QuickGamma").as_ptr(),
                    MB_OK,
                );
            }
            _ => (),
        }
        Ok(())
    }
    unsafe fn tray(&mut self, h: HWND, op: u32) -> bool {
        let mut n: NOTIFYICONDATAW = std::mem::zeroed();
        n.cbSize = std::mem::size_of_val(&n) as _;
        n.hWnd = h;
        n.uID = 1;
        n.uCallbackMessage = TRAY;
        n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        n.hIcon = LoadIconW(GetModuleHandleW(null()), 101 as _);
        let tip = wide("QuickGamma");
        n.szTip[..tip.len()].copy_from_slice(&tip);
        Shell_NotifyIconW(op, &n) != 0
    }
    unsafe fn handle(&mut self, h: HWND, msg: u32, w: usize, l: isize) -> Result<isize> {
        match msg {
            WM_COPYDATA => {
                if l == 0 {
                    return Ok(0);
                }
                let data = &*(l as *const COPYDATASTRUCT);
                if data.dwData != MAGIC
                    || data.cbData == 0
                    || data.cbData > 32768
                    || data.lpData.is_null()
                {
                    return Ok(0);
                }
                let bytes =
                    std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize);
                if bytes == b"OPEN" {
                    self.open(h)
                } else {
                    let s = std::str::from_utf8(bytes).map_err(|_| err("引数が不正です"))?;
                    let args = s.split('\n').map(str::to_string).collect::<Vec<_>>();
                    self.refresh(h);
                    self.batch(&parse(&args, self.screens.len())?)?;
                    self.before = self.values.clone();
                    self.sync(h);
                }
                SetWindowLongPtrW(h, 0, 1);
            }
            WM_COMMAND => self.command(h, (w & 65535) as i32, (w >> 16) as u32)?,
            WM_DISPLAYCHANGE => {
                self.refresh(h);
                for d in &self.screens {
                    let k = token(d);
                    if let Some(&v) = self.values.get(&k) {
                        if v != 100 {
                            self.worker.set(k, v)?;
                        }
                    }
                }
            }
            WM_HSCROLL if l == GetDlgItem(h, SLIDER) as isize => {
                if let Some(i) = self.selected(h) {
                    let pos = SendDlgItemMessageW(h, SLIDER, TBM_GETPOS, 0, 0);
                    self.set(i, pos as u16)?;
                    self.sync(h);
                }
            }
            WM_CLOSE => {
                if self.tray_ok {
                    self.command(h, 2, 0)?
                } else {
                    PostQuitMessage(0)
                }
            }
            WM_ENDSESSION if w != 0 => PostQuitMessage(0),
            TRAY => match l as u32 {
                WM_LBUTTONUP => self.open(h),
                WM_RBUTTONUP => {
                    let menu = CreatePopupMenu();
                    if menu.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    AppendMenuW(menu, 0, 1, wide("開く").as_ptr());
                    AppendMenuW(menu, 0, 2, wide("終了して元に戻す").as_ptr());
                    let mut p = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut p);
                    SetForegroundWindow(h);
                    let selected = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_RIGHTBUTTON,
                        p.x,
                        p.y,
                        0,
                        h,
                        null(),
                    );
                    DestroyMenu(menu);
                    if selected == 1 {
                        self.open(h)
                    } else if selected == 2 {
                        PostQuitMessage(0)
                    }
                }
                _ => (),
            },
            m if m == self.restart => {
                self.tray_ok = self.tray(h, NIM_ADD);
                if !self.tray_ok {
                    self.open(h)
                }
            }
            _ => return Ok(0),
        }
        Ok(1)
    }
}
unsafe extern "system" fn dialog(h: HWND, m: u32, w: usize, l: isize) -> isize {
    APP.with(|cell| {
        let Ok(mut state) = cell.try_borrow_mut() else {
            return 0;
        };
        let Some(app) = state.as_mut() else { return 0 };
        match app.handle(h, m, w, l) {
            Ok(v) => v,
            Err(e) => {
                app.sync(h);
                if m == WM_COPYDATA {
                    SetWindowLongPtrW(h, 0, 0);
                } else {
                    alert(&e.to_string());
                }
                1
            }
        }
    })
}
unsafe fn forward(args: &[String]) -> Result<()> {
    let payload = if args.is_empty() {
        "OPEN".to_owned()
    } else {
        args.join("\n")
    };
    for _ in 0..100 {
        let mut h = null_mut();
        loop {
            h = FindWindowExW(
                null_mut(),
                h,
                wide("#32770").as_ptr(),
                wide("QuickGamma").as_ptr(),
            );
            if h.is_null() {
                break;
            }
            if GetPropW(h, wide("QuickGamma.Ready.2").as_ptr()).is_null() {
                continue;
            }
            let data = COPYDATASTRUCT {
                dwData: MAGIC,
                cbData: payload.len() as _,
                lpData: payload.as_ptr() as _,
            };
            let mut result = 0;
            if SendMessageTimeoutW(
                h,
                WM_COPYDATA,
                0,
                &data as *const _ as _,
                SMTO_ABORTIFHUNG,
                10000,
                &mut result,
            ) == 0
                || result != 1
            {
                return Err(err("常駐中の QuickGamma が要求を適用できませんでした"));
            }
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(err("常駐中の画面が見つかりません"))
}
pub fn run(args: &[String]) -> Result<()> {
    unsafe {
        let Some(_lock) = acquire_mutex("Local\\QuickGammaInstanceMutex", 0)? else {
            return forward(args);
        };
        InitCommonControls();
        let instance = GetModuleHandleW(null());
        let h = CreateDialogParamW(instance, 102 as _, null_mut(), Some(dialog), 0);
        if h.is_null() {
            return Err(io::Error::last_os_error());
        }
        let icon = LoadIconW(instance, 101 as _);
        for size in [0, 1] {
            SendMessageW(h, WM_SETICON, size, icon as isize);
        }
        let mica = 2i32;
        DwmSetWindowAttribute(h, 38, &mica as *const _ as _, 4);
        SendDlgItemMessageW(
            h,
            SLIDER,
            TBM_SETRANGE,
            1,
            ((MAX_GAMMA as isize) << 16) | MIN_GAMMA as isize,
        );
        SendDlgItemMessageW(h, EDIT, EM_SETLIMITTEXT, 16, 0);
        let mut app = App {
            worker: Worker::start()?,
            screens: Vec::new(),
            values: BTreeMap::new(),
            before: BTreeMap::new(),
            restart: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            tray_ok: false,
        };
        app.refresh(h);
        app.batch(&parse(args, app.screens.len())?)?;
        app.tray_ok = app.tray(h, NIM_ADD);
        if args.is_empty() || !app.tray_ok {
            app.open(h);
        }
        APP.with(|c| *c.borrow_mut() = Some(app));
        SetPropW(h, wide("QuickGamma.Ready.2").as_ptr(), 1 as _);
        let mut msg: MSG = std::mem::zeroed();
        let result = loop {
            match GetMessageW(&mut msg, null_mut(), 0, 0) {
                -1 => break Err(io::Error::last_os_error()),
                0 => break Ok(()),
                _ => {
                    if IsDialogMessageW(h, &msg) == 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }
        };
        let mut app = APP.with(|c| c.borrow_mut().take().unwrap());
        app.tray(h, NIM_DELETE);
        DestroyWindow(h);
        app.worker.finish()?;
        result
    }
}
