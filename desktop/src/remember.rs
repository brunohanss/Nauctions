//! A remembered member credential is bound to its server and Windows user.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Serialize, Deserialize)]
struct Saved {
    origin: String,
    token: String,
}
fn path(data: &Path, origin: &str) -> PathBuf {
    data.join("sessions")
        .join(format!("{:x}.dpapi", Sha256::digest(origin.as_bytes())))
}
fn valid(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}
#[cfg(windows)]
fn protect(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    unsafe {
        let success = if decrypt {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if success == 0 {
            return Err("Cannot protect or restore this computer's membership.".into());
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as *mut _);
        Ok(result)
    }
}
#[cfg(not(windows))]
fn protect(_bytes: &[u8], _decrypt: bool) -> Result<Vec<u8>, String> {
    Err("Remembered membership requires Windows DPAPI.".into())
}
pub fn load(data: &Path, origin: &str) -> Result<Option<String>, String> {
    let file = path(data, origin);
    if !file.exists() {
        return Ok(None);
    }
    if fs::metadata(&file)
        .map_err(|_| "Cannot read remembered membership.")?
        .len()
        > 16384
    {
        return Err("Invalid remembered membership.".into());
    }
    let saved: Saved = serde_json::from_slice(&protect(
        &fs::read(file).map_err(|_| "Cannot read remembered membership.")?,
        true,
    )?)
    .map_err(|_| "Invalid remembered membership.")?;
    if saved.origin != origin || !valid(&saved.token) {
        return Err("Remembered membership does not match this server.".into());
    }
    Ok(Some(saved.token))
}
pub fn save(data: &Path, origin: &str, token: &str) -> Result<(), String> {
    if !valid(token) {
        return Err("Invalid member credential returned by the server.".into());
    }
    let destination = path(data, origin);
    fs::create_dir_all(destination.parent().unwrap())
        .map_err(|_| "Cannot create membership storage.")?;
    let bytes = protect(
        &serde_json::to_vec(&Saved {
            origin: origin.into(),
            token: token.into(),
        })
        .map_err(|_| "Cannot encode membership.")?,
        false,
    )?;
    let temporary = destination.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Invalid system time.")?
            .as_nanos()
    ));
    fs::write(&temporary, bytes).map_err(|_| "Cannot save remembered membership.")?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::*;
        let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            let _ = fs::remove_file(temporary);
            return Err("Cannot commit remembered membership.".into());
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(temporary, destination).map_err(|_| "Cannot save membership.")?;
    }
    Ok(())
}
pub fn forget(data: &Path, origin: &str) -> Result<(), String> {
    match fs::remove_file(path(data, origin)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cannot remove remembered membership.".into()),
    }
}
pub fn client(data: &Path, origin: Option<&str>) -> Result<reqwest::Client, String> {
    let token = origin.and_then(|o| load(data, o).ok().flatten());
    crate::transport::client_with_session(origin, token.as_deref())
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn remembered_membership_is_encrypted_atomic_and_server_bound() {
        let dir = tempfile::tempdir().unwrap();
        let token = "a".repeat(64);
        save(dir.path(), "https://one.example", &token).unwrap();
        assert_eq!(
            load(dir.path(), "https://one.example").unwrap(),
            Some(token.clone())
        );
        assert_eq!(load(dir.path(), "https://two.example").unwrap(), None);
        assert!(!String::from_utf8_lossy(
            &fs::read(path(dir.path(), "https://one.example")).unwrap()
        )
        .contains(&token));
        let other = "b".repeat(64);
        save(dir.path(), "https://one.example", &other).unwrap();
        assert_eq!(
            load(dir.path(), "https://one.example").unwrap(),
            Some(other)
        );
        fs::copy(
            path(dir.path(), "https://one.example"),
            path(dir.path(), "https://two.example"),
        )
        .unwrap();
        assert!(load(dir.path(), "https://two.example").is_err());
        forget(dir.path(), "https://one.example").unwrap();
        assert_eq!(load(dir.path(), "https://one.example").unwrap(), None);
    }
    #[test]
    fn a_new_native_client_restores_the_saved_member_cookie() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let dir = tempfile::tempdir().unwrap();
        let token = "c".repeat(64);
        save(dir.path(), &origin, &token).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut b = [0];
                stream.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
                if bytes.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            assert!(String::from_utf8(bytes)
                .unwrap()
                .contains(&format!("market_session={token}")));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            )
            .unwrap();
        });
        tauri::async_runtime::block_on(async {
            let response = crate::transport::request(
                client(dir.path(), Some(&origin)).unwrap(),
                origin,
                "/me".into(),
                "GET".into(),
                None,
                None,
            )
            .await
            .unwrap();
            assert_eq!(response.status, 200);
        });
        server.join().unwrap();
    }
}
