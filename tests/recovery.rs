use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    System::Threading::{
        GetExitCodeProcess, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE,
    },
};

// テスト用の親だけを強制終了し、実画面への書き込みは行わない。
#[test]
#[ignore = "親プロセスの強制終了テストから起動"]
fn pipe_owner() {
    let Some(path) = std::env::var_os("QUICKGAMMA_TEST_PARENT") else {
        return;
    };
    let dir = std::path::Path::new(&path);
    let mut worker = Command::new(env!("CARGO_BIN_EXE_QuickGamma"))
        .arg("--restore-worker")
        .env("LOCALAPPDATA", dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(worker.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "OK");
    writeln!(worker.stdin.as_mut().unwrap(), "0 999").unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    assert!(line.starts_with("ERR "));
    fs::write(dir.join("pid.tmp"), worker.id().to_string()).unwrap();
    fs::rename(dir.join("pid.tmp"), dir.join("ready")).unwrap();
    thread::sleep(Duration::from_secs(30));
    drop(worker.stdin.take());
    worker.wait().unwrap();
}
#[test]
fn worker_detects_terminated_parent() {
    let dir = std::env::temp_dir().join(format!("quickgamma-pipe-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "pipe_owner", "--nocapture"])
        .env("QUICKGAMMA_TEST_PARENT", &dir)
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !dir.join("ready").exists() {
        if parent.try_wait().unwrap().is_some() {
            panic!("テスト用の親が終了しました")
        }
        if start.elapsed() > Duration::from_secs(15) {
            parent.kill().unwrap();
            parent.wait().unwrap();
            panic!("復元プロセスの起動がタイムアウトしました")
        }
        thread::sleep(Duration::from_millis(20));
    }
    let id = fs::read_to_string(dir.join("ready"))
        .unwrap()
        .parse()
        .unwrap();
    unsafe {
        let handle = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            id,
        );
        assert!(!handle.is_null());
        parent.kill().unwrap();
        parent.wait().unwrap();
        let wait = WaitForSingleObject(handle, 5000);
        let mut code = 1;
        let queried = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        assert_eq!(wait, WAIT_OBJECT_0);
        assert_ne!(queried, 0);
        assert_eq!(code, 0);
    }
    fs::remove_dir_all(dir).unwrap();
}
