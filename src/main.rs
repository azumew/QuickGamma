#![windows_subsystem = "windows"]
mod gamma;
mod ui;
use std::{
    io::{self, BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    ptr::null,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{RemoteDesktop::ProcessIdToSessionId, Threading::*},
    UI::WindowsAndMessaging::*,
};
type Result<T> = io::Result<T>;
fn err(msg: &str) -> io::Error {
    io::Error::other(msg)
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Mutex(HANDLE);
impl Drop for Mutex {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}
fn acquire_mutex(name: &str, timeout: u32) -> Result<Option<Mutex>> {
    unsafe {
        let h = CreateMutexW(null(), 0, wide(name).as_ptr());
        if h.is_null() {
            return Err(io::Error::last_os_error());
        }
        match WaitForSingleObject(h, timeout) {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Some(Mutex(h))),
            WAIT_TIMEOUT => {
                CloseHandle(h);
                Ok(None)
            }
            _ => {
                let e = io::Error::last_os_error();
                CloseHandle(h);
                Err(e)
            }
        }
    }
}
fn session_id() -> Result<u32> {
    let mut id = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut id) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(id)
}
fn value(s: &str) -> Result<u16> {
    let x = s.trim().parse::<f64>().unwrap_or(f64::NAN);
    if !(0.2..=4.).contains(&x) {
        return Err(err("ガンマは 0.20 ～ 4.00 で指定してください"));
    }
    Ok((x * 100.).round() as u16)
}
fn parse(args: &[String], count: usize) -> Result<Vec<(usize, u16)>> {
    let mut result = Vec::new();
    let mut target = None;
    let mut pending = false;
    for pair in args.chunks(2) {
        let [flag, arg] = pair else {
            return Err(err("引数の値がありません"));
        };
        match flag.as_str() {
            "-m" | "/m" => {
                let index = arg
                    .parse::<usize>()
                    .ok()
                    .filter(|i| (1..=count).contains(i))
                    .ok_or_else(|| err("モニター番号が範囲外です"))?;
                target = Some(index - 1);
                pending = true;
            }
            "-g" | "/g" => {
                let v = value(arg)?;
                if let Some(i) = target {
                    result.push((i, v))
                } else {
                    result.extend((0..count).map(|i| (i, v)))
                }
                pending = false;
            }
            _ => return Err(err("未対応の引数です")),
        }
    }
    if pending || (!args.is_empty() && result.is_empty()) {
        return Err(err("適用対象またはガンマ値がありません"));
    }
    Ok(result)
}
struct Worker {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}
impl Worker {
    fn start() -> Result<Self> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--restore-worker")
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut w = Self {
            child,
            input,
            output,
        };
        w.reply()?;
        Ok(w)
    }
    fn reply(&mut self) -> Result<()> {
        let mut line = String::new();
        self.output.read_line(&mut line)?;
        match line.trim_end() {
            "OK" => Ok(()),
            "" => Err(err(
                "復元プロセスが停止しました。QuickGamma を再起動してください",
            )),
            msg => Err(err(msg.trim_start_matches("ERR "))),
        }
    }
    fn set(&mut self, key: u64, val: u16) -> Result<()> {
        writeln!(self.input.as_mut().unwrap(), "{key} {val}")?;
        self.reply()
    }
    fn finish(&mut self) -> Result<()> {
        drop(self.input.take());
        if self.child.wait()?.success() {
            Ok(())
        } else {
            Err(err("復元が完了しませんでした。次回起動時に再試行します"))
        }
    }
}
fn shortcut(args: &str) -> Result<()> {
    let script = r#"$ErrorActionPreference='Stop'; $shell=New-Object -ComObject WScript.Shell; $dir=[Environment]::GetFolderPath('DesktopDirectory'); $name='QuickGamma_'+(Get-Date -Format yyyyMMdd_HHmmss_fffffff)+'.lnk'; $link=$shell.CreateShortcut([IO.Path]::Combine($dir,$name)); $link.TargetPath=$env:QG_EXE; $link.Arguments=$env:QG_ARGS; $link.Save()"#;
    let ps = std::path::PathBuf::from(
        std::env::var_os("SystemRoot").ok_or_else(|| err("Windows フォルダーが見つかりません"))?,
    )
    .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let status = Command::new(ps)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("QG_EXE", std::env::current_exe()?)
        .env("QG_ARGS", args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(err("ショートカットを保存できません"))
    }
}
fn alert(s: &str) {
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wide(s).as_ptr(),
            wide("QuickGamma").as_ptr(),
            MB_ICONERROR,
        );
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--restore-worker"] {
        if let Err(e) = gamma::worker() {
            println!("ERR {e}");
            std::process::exit(1)
        }
        return;
    }
    if let Err(e) = ui::run(&args) {
        alert(&e.to_string());
        std::process::exit(1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_cli() {
        let a = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            parse(&a("/g 1.2 -m 2 -g .8"), 2).unwrap(),
            [(0, 120), (1, 120), (1, 80)]
        );
        for s in [
            "-m 0 -g 1",
            "-g NaN",
            "-g inf",
            "-g 1 -m 3",
            "-g",
            "-g 4.01",
            "-m 1",
        ] {
            assert!(parse(&a(s), 2).is_err())
        }
    }
}
