use crate::{err, wide, Result};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
use windows_sys::Win32::Graphics::Gdi::*;

type Ramp = [[u16; 256]; 3];
#[link(name = "gdi32")]
extern "system" {
    fn GetDeviceGammaRamp(dc: HDC, ramp: *mut Ramp) -> i32;
    fn SetDeviceGammaRamp(dc: HDC, ramp: *const Ramp) -> i32;
}
#[derive(Clone)]
pub struct Display {
    pub name: String,
    pub id: String,
}
fn string(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..buf.iter().position(|v| *v == 0).unwrap_or(buf.len())])
}
pub fn displays() -> Vec<Display> {
    let mut list = Vec::new();
    unsafe {
        for i in 0.. {
            let mut adapter: DISPLAY_DEVICEW = std::mem::zeroed();
            adapter.cb = std::mem::size_of_val(&adapter) as _;
            if EnumDisplayDevicesW(std::ptr::null(), i, &mut adapter, 0) == 0 {
                break;
            }
            if adapter.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
                continue;
            }
            let mut ids = Vec::new();
            for j in 0.. {
                let mut monitor: DISPLAY_DEVICEW = std::mem::zeroed();
                monitor.cb = std::mem::size_of_val(&monitor) as _;
                if EnumDisplayDevicesW(adapter.DeviceName.as_ptr(), j, &mut monitor, 1) == 0 {
                    break;
                }
                if monitor.StateFlags & DISPLAY_DEVICE_ACTIVE != 0 {
                    ids.push(string(&monitor.DeviceID))
                }
            }
            ids.sort();
            if !ids.is_empty() && ids.iter().all(|s| !s.is_empty()) {
                list.push(Display {
                    name: string(&adapter.DeviceName),
                    id: ids.join("|"),
                });
            }
        }
    }
    list
}
impl Display {
    fn dc<T>(&self, f: impl FnOnce(HDC) -> Result<T>) -> Result<T> {
        unsafe {
            let dc = CreateDCW(
                wide("DISPLAY").as_ptr(),
                wide(&self.name).as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            );
            if dc.is_null() {
                return Err(err("画面を開けません"));
            }
            let result = f(dc);
            DeleteDC(dc);
            result
        }
    }
    fn read(&self) -> Result<Ramp> {
        self.dc(|dc| {
            let mut r = [[0; 256]; 3];
            if unsafe { GetDeviceGammaRamp(dc, &mut r) } == 0 {
                Err(err("元の補正を取得できません"))
            } else {
                Ok(r)
            }
        })
    }
    fn write(&self, r: &Ramp) -> Result<()> {
        self.dc(|dc| {
            if unsafe { SetDeviceGammaRamp(dc, r) } == 0 {
                return Err(err("ドライバーが補正を拒否しました"));
            }
            let mut actual = [[0; 256]; 3];
            if unsafe { GetDeviceGammaRamp(dc, &mut actual) } == 0 {
                return Err(err("適用結果を読み戻せません"));
            }
            // 一部の機器では下位 8 ビットが丸められる。
            if actual
                .iter()
                .flatten()
                .zip(r.iter().flatten())
                .any(|(a, b)| a.abs_diff(*b) > 256)
            {
                return Err(err("画面から読み戻した補正値が一致しません"));
            }
            Ok(())
        })
    }
}
fn curve(base: &Ramp, value: u16) -> Ramp {
    if value == 100 {
        return *base;
    }
    let mut out = [[0; 256]; 3];
    for (src, dst) in base.iter().zip(&mut out) {
        for (i, v) in dst.iter_mut().enumerate() {
            let x = (i as f64 / 255.).powf(100. / value as f64) * 255.;
            let lo = x.floor() as usize;
            let t = x - lo as f64;
            *v = (src[lo] as f64 * (1. - t) + src[(lo + 1).min(255)] as f64 * t).round() as u16;
        }
    }
    out
}
fn key(id: &str) -> u64 {
    id.bytes().fold(14695981039346656037, |h, b| {
        (h ^ b as u64).wrapping_mul(1099511628211)
    })
}
pub fn token(d: &Display) -> u64 {
    key(&d.id)
}
fn backup(dir: &Path, id: &str, r: &Ramp) -> Result<PathBuf> {
    let path = dir.join(format!("{:016x}.ramp", key(id)));
    if path.exists() {
        return Err(err("前回の復元が保留中です"));
    }
    let tmp = path.with_extension("tmp");
    let mut f = io::BufWriter::new(fs::File::create(&tmp)?);
    writeln!(f, "{id}")?;
    for ch in r {
        for v in ch {
            f.write_all(&v.to_le_bytes())?
        }
    }
    f.flush()?;
    f.get_ref().sync_all()?;
    drop(f);
    fs::rename(tmp, &path)?;
    Ok(path)
}
fn decode(bytes: &[u8]) -> Result<(&str, Ramp)> {
    let n = bytes
        .iter()
        .position(|v| *v == 10)
        .ok_or_else(|| err("復元ファイルが不正です"))?;
    let id = std::str::from_utf8(&bytes[..n]).map_err(|_| err("画面識別子が不正です"))?;
    let data = &bytes[n + 1..];
    if data.len() != 1536 {
        return Err(err("補正テーブルの長さが不正です"));
    }
    let mut ramp = [[0; 256]; 3];
    for (dst, src) in ramp.iter_mut().flatten().zip(data.as_chunks::<2>().0) {
        *dst = u16::from_le_bytes([src[0], src[1]])
    }
    Ok((id, ramp))
}
fn recover(dir: &Path) -> Result<()> {
    let screens = displays();
    recover_with(dir, |id, r| {
        if let Some(d) = screens.iter().find(|d| d.id == id) {
            d.write(r)?;
            Ok(true)
        } else {
            Ok(false)
        }
    })
}
fn recover_with(dir: &Path, mut restore: impl FnMut(&str, &Ramp) -> Result<bool>) -> Result<()> {
    let mut failure = None;
    for item in fs::read_dir(dir)? {
        let p = item?.path();
        if p.extension().and_then(|s| s.to_str()) != Some("ramp") {
            continue;
        }
        let restored = (|| -> Result<()> {
            let bytes = fs::read(&p)?;
            let (id, r) = decode(&bytes)?;
            if restore(id, &r)? {
                fs::remove_file(p)?
            }
            Ok(())
        })();
        if let Err(e) = restored {
            failure = Some(e)
        }
    }
    failure.map_or(Ok(()), Err)
}
struct Original {
    ramp: Ramp,
    path: PathBuf,
}
fn change(dir: &Path, originals: &mut BTreeMap<String, Original>, line: &str) -> Result<()> {
    let (k, v) = line.split_once(' ').ok_or_else(|| err("要求が不正です"))?;
    let k: u64 = k.parse().map_err(|_| err("画面番号が不正です"))?;
    let v: u16 = v.parse().map_err(|_| err("補正値が不正です"))?;
    if !(crate::MIN_GAMMA..=crate::MAX_GAMMA).contains(&v) {
        return Err(err("補正値が範囲外です"));
    }
    let d = displays()
        .into_iter()
        .find(|d| token(d) == k)
        .ok_or_else(|| err("画面の接続が変わりました"))?;
    if !originals.contains_key(&d.id) {
        if v == 100 {
            return Ok(());
        }
        let ramp = d.read()?;
        let path = backup(dir, &d.id, &ramp)?;
        originals.insert(d.id.clone(), Original { ramp, path });
    }
    let original = &originals[&d.id];
    let previous = d.read()?;
    if let Err(e) = d.write(&curve(&original.ramp, v)) {
        if d.write(&previous).is_err() {
            return Err(err(&format!(
                "{e}。直前の状態への復元も失敗しました。終了時に再試行します"
            )));
        }
        return Err(e);
    }
    if v == 100 {
        fs::remove_file(&original.path)?;
        originals.remove(&d.id);
    }
    Ok(())
}
pub fn worker() -> Result<()> {
    let dir = PathBuf::from(
        std::env::var_os("LOCALAPPDATA").ok_or_else(|| err("保存先を取得できません"))?,
    )
    .join("QuickGamma")
    .join(format!("recovery-{}", crate::session_id()?));
    let name = format!(
        "Local\\QuickGamma.Restore.{:016x}",
        key(&dir.to_string_lossy())
    );
    let _lock =
        crate::acquire_mutex(&name, 10000)?.ok_or_else(|| err("別の復元処理が動作中です"))?;
    fs::create_dir_all(&dir)?;
    recover(&dir)?;
    let mut originals = BTreeMap::new();
    let serve = (|| -> Result<()> {
        let mut output = io::stdout().lock();
        writeln!(output, "OK")?;
        output.flush()?;
        for line in io::stdin().lock().lines() {
            let result = change(&dir, &mut originals, &line?);
            match result {
                Ok(()) => writeln!(output, "OK")?,
                Err(e) => writeln!(output, "ERR {e}")?,
            }
            output.flush()?;
        }
        Ok(())
    })();
    for attempt in 0..3 {
        match recover(&dir) {
            Ok(()) => break,
            Err(e) if attempt == 2 => return Err(e),
            _ => thread::sleep(Duration::from_millis(250)),
        }
    }
    serve
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calibrated_identity() {
        let base = std::array::from_fn(|c| std::array::from_fn(|i| (i * (240 + c)) as u16));
        assert_eq!(curve(&base, 100), base);
        let bright = curve(&base, 150);
        assert!(bright[0][100] > base[0][100]);
        assert_eq!(bright[2][255], base[2][255]);
    }
    #[test]
    fn interrupted_session_recovery() {
        let dir =
            std::env::temp_dir().join(format!("quickgamma-recovery-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let a = backup(&dir, "A", &[[33; 256]; 3]).unwrap();
        let b = backup(&dir, "B", &[[77; 256]; 3]).unwrap();
        assert!(recover_with(&dir, |id, _| if id == "A" {
            Err(err("書込失敗"))
        } else {
            Ok(false)
        })
        .is_err());
        assert!(a.exists() && b.exists());
        recover_with(&dir, |id, r| {
            assert_eq!(r[0][0], if id == "A" { 33 } else { 77 });
            Ok(id == "A")
        })
        .unwrap();
        assert!(!a.exists() && b.exists());
        recover_with(&dir, |id, r| {
            assert_eq!(id, "B");
            assert_eq!(r[0][0], 77);
            Ok(true)
        })
        .unwrap();
        assert!(!b.exists());
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn backup_roundtrip() {
        let dir = std::env::temp_dir().join(format!("quickgamma-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let r = [[321; 256]; 3];
        let path = backup(&dir, "モニター", &r).unwrap();
        assert!(backup(&dir, "モニター", &r).is_err());
        let bytes = fs::read(&path).unwrap();
        assert_eq!(decode(&bytes).unwrap(), ("モニター", r));
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
