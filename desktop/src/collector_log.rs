use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Child,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

const LIMIT: u64 = 5 * 1024 * 1024;

pub struct Log {
    path: PathBuf,
    file: Option<File>,
    length: u64,
    limit: u64,
}

impl Log {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        Self::with_limit(path, LIMIT)
    }

    fn with_limit(path: &Path, limit: u64) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let length = file.metadata()?.len();
        let mut log = Self {
            path: path.into(),
            file: Some(file),
            length,
            limit,
        };
        if length > limit {
            // Discard an old oversized log without retaining an oversized backup.
            log.file.take();
            OpenOptions::new().write(true).truncate(true).open(path)?;
            log.file = Some(OpenOptions::new().append(true).open(path)?);
            log.length = 0;
        }
        Ok(log)
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        self.file.take();
        let backup = self.path.with_extension("log.1");
        match fs::remove_file(&backup) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&self.path, backup)?;
        self.file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?,
        );
        self.length = 0;
        Ok(())
    }

    fn write(&mut self, mut bytes: &[u8]) -> std::io::Result<()> {
        while !bytes.is_empty() {
            if self.length >= self.limit || self.file.is_none() {
                self.rotate()?;
            }
            let count = bytes.len().min((self.limit - self.length) as usize);
            let file = self.file.as_mut().unwrap();
            if let Err(error) = file.write_all(&bytes[..count]) {
                // A failed write can still append a prefix. Include it in the
                // bound before handling any later output.
                self.length = file
                    .metadata()
                    .map(|metadata| metadata.len())
                    .unwrap_or(self.limit);
                return Err(error);
            }
            self.length += count as u64;
            bytes = &bytes[count..];
        }
        Ok(())
    }
}

pub struct Capture {
    threads: Vec<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}

impl Capture {
    pub fn attach(child: &mut Child, log: Log) -> Result<Self, String> {
        let stdout = child
            .stdout
            .take()
            .ok_or("Collector output pipe is missing.")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Collector error pipe is missing.")?;
        let sink = Arc::new(Mutex::new(log));
        let error = Arc::new(Mutex::new(None));
        let threads = vec![
            drain(stdout, sink.clone(), error.clone()),
            drain(stderr, sink, error.clone()),
        ];
        Ok(Self { threads, error })
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|error| error.clone())
    }

    pub fn finish(self) -> Option<String> {
        for thread in self.threads {
            let _ = thread.join();
        }
        self.error.lock().ok().and_then(|error| error.clone())
    }
}

fn drain(
    mut input: impl Read + Send + 'static,
    sink: Arc<Mutex<Log>>,
    error: Arc<Mutex<Option<String>>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut bytes = [0; 8192];
        loop {
            let count = match input.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => count,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    if let Ok(mut error) = error.lock() {
                        *error = Some("Cannot read collector diagnostics.".into());
                    }
                    break;
                }
            };
            let written = sink
                .lock()
                .map_err(|_| ())
                .and_then(|mut log| log.write(&bytes[..count]).map_err(|_| ()));
            if written.is_err() {
                if let Ok(mut error) = error.lock() {
                    *error = Some("Cannot save collector diagnostics. Check free disk space and folder access.".into());
                }
                // Keep draining both pipes so a logging failure cannot block uploads.
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_helper() {
        if std::env::var_os("NAUCTIONS_LOG_PIPE_TEST").is_some() {
            for _ in 0..1000 {
                std::io::stdout().write_all(&[b'o'; 1024]).unwrap();
                std::io::stderr().write_all(&[b'e'; 1024]).unwrap();
            }
        }
    }

    #[test]
    fn live_child_output_rotates_without_blocking_either_pipe() {
        use std::{
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collector.log");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "collector_log::tests::pipe_helper",
                "--nocapture",
            ])
            .env("NAUCTIONS_LOG_PIPE_TEST", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().unwrap();
        let capture = Capture::attach(&mut child, Log::with_limit(&path, 4096).unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                break None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(capture.finish().is_none());
        assert!(
            status.is_some_and(|status| status.success()),
            "both pipes must drain before the deadline"
        );
        assert!(fs::metadata(&path).unwrap().len() <= 4096);
        assert!(fs::metadata(path.with_extension("log.1")).unwrap().len() <= 4096);
    }

    #[test]
    fn continuous_output_keeps_only_two_bounded_logs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collector.log");
        let mut log = Log::with_limit(&path, 8).unwrap();
        log.write(b"12345678abcdefghABCDEFGHijkl").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"ijkl");
        assert_eq!(fs::read(path.with_extension("log.1")).unwrap(), b"ABCDEFGH");
        drop(log);
        let mut log = Log::with_limit(&path, 8).unwrap();
        log.write(b"mnop").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"ijklmnop");
    }

    #[test]
    fn failed_logging_still_consumes_the_output_pipe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collector.log");
        let mut log = Log::with_limit(&path, 8).unwrap();
        log.write(b"12345678").unwrap();
        fs::create_dir(path.with_extension("log.1")).unwrap();
        let sink = Arc::new(Mutex::new(log));
        let error = Arc::new(Mutex::new(None));
        drain(std::io::Cursor::new(vec![b'x'; 32768]), sink, error.clone())
            .join()
            .unwrap();
        assert!(error.lock().unwrap().is_some());
        assert_eq!(fs::metadata(path).unwrap().len(), 8);
    }
}
