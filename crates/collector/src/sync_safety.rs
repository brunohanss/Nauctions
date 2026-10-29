//! Installation-scoped coordination and local recovery. Never needs a server credential.
use super::*;
use std::io::Write;

const TERMINAL: [&str; 4] = ["committed", "original_retained", "restored", "cancelled"];
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub id: Uuid,
    pub installation: String,
    pub source: PathBuf,
    pub owner: PathBuf,
    pub original_hash: String,
    pub output_hash: String,
    pub original_size: u64,
    pub output_size: u64,
    pub created_at: i64,
    pub kind: String,
}
pub struct Coordinator {
    pub root: PathBuf,
    home: PathBuf,
    identity: String,
    db: Connection,
    _lock: fs::File,
    _root_handle: fs::File,
}
fn check_components(path: &Path) -> Result<()> {
    let mut cursor = path.to_path_buf();
    loop {
        if is_link(&cursor) {
            bail!("links and reparse points are refused");
        }
        if !cursor.pop() {
            break;
        }
    }
    Ok(())
}
#[cfg(windows)]
fn root_handle(root: &Path) -> Result<(fs::File, String)> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::*;
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(root)?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let mut fs_name = [0u16; 32];
    unsafe {
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0
            || GetVolumeInformationByHandleW(
                file.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs_name.as_mut_ptr(),
                32,
            ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    let n = fs_name
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(fs_name.len());
    if String::from_utf16_lossy(&fs_name[..n]) != "NTFS" {
        bail!("history writes require a local NTFS installation");
    }
    Ok((
        file,
        format!(
            "{:08x}-{:08x}{:08x}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ),
    ))
}
#[cfg(not(windows))]
fn root_handle(_: &Path) -> Result<(fs::File, String)> {
    bail!("history writes require Windows");
}

impl Coordinator {
    pub fn open(root: &Path, create: bool) -> Result<Option<Self>> {
        check_components(root)?;
        let root = root.canonicalize()?;
        let text = root.to_string_lossy();
        if text.starts_with(r"\\?\UNC\") || (text.starts_with(r"\\") && !text.starts_with(r"\\?\"))
        {
            bail!("network installations are not supported for writes");
        }
        let home = root.join("WTF/.nauctions-sync");
        check_components(&home)?;
        if !home.exists() && !create {
            return Ok(None);
        }
        if create {
            fs::create_dir_all(&home)?;
        }
        let (root_file, identity) = root_handle(&root)?;
        let lock_path = home.join("writer.lock");
        check_components(&lock_path)?;
        let mut options = fs::OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(create)
            .truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let lock = options
            .open(&lock_path)
            .context("installation_busy: cannot acquire the installation writer lock")?;
        for name in ["journal.sqlite", "journal.sqlite-wal", "journal.sqlite-shm"] {
            check_components(&home.join(name))?;
        }
        let db = Connection::open(home.join("journal.sqlite"))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS identity(id TEXT PRIMARY KEY); CREATE TABLE IF NOT EXISTS operations(id TEXT PRIMARY KEY,document TEXT NOT NULL,phase TEXT NOT NULL,error TEXT);")?;
        let previous: Option<String> = db
            .query_row("SELECT id FROM identity", [], |r| r.get(0))
            .optional()?;
        if previous.as_ref().is_some_and(|old| old != &identity) {
            bail!("installation identity changed; recovery review required");
        }
        db.execute("INSERT OR IGNORE INTO identity VALUES(?)", [&identity])?;
        Ok(Some(Self {
            root,
            home,
            identity,
            db,
            _lock: lock,
            _root_handle: root_file,
        }))
    }
    fn target(&self, op: &Operation) -> Result<PathBuf> {
        if op.installation != self.identity {
            bail!("operation belongs to another installation");
        }
        let parts: Vec<_> = op.source.components().collect();
        if parts.len() != 5
            || parts
                .iter()
                .any(|p| !matches!(p, std::path::Component::Normal(_)))
            || parts[0].as_os_str() != "WTF"
            || parts[1].as_os_str() != "Account"
            || parts[3].as_os_str() != "SavedVariables"
            || parts[4].as_os_str() != "AuctionDB.lua"
            || parts[2]
                .as_os_str()
                .to_string_lossy()
                .contains([':', '/', '\\'])
        {
            bail!("invalid recovery account path");
        }
        for h in [&op.original_hash, &op.output_hash] {
            if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("invalid recovery digest");
            }
        }
        if op.original_size > MAX_FILE || op.output_size > MAX_FILE {
            bail!("invalid recovery size");
        }
        let target = self.root.join(&op.source);
        check_components(&target)?;
        if !target
            .parent()
            .unwrap()
            .canonicalize()?
            .starts_with(&self.root)
        {
            bail!("recovery target escaped installation");
        }
        Ok(target)
    }
    fn artifact(&self, op: &Operation, suffix: &str) -> Result<PathBuf> {
        let source = self.target(op)?;
        let path = source.with_file_name(format!(".nauctions-{}.{suffix}", op.id));
        check_components(&path)?;
        Ok(path)
    }
    fn private(&self, op: &Operation) -> Result<PathBuf> {
        check_components(&op.owner)?;
        if op.owner.canonicalize()?.starts_with(&self.root) {
            bail!("private backup directory is inside game installation");
        }
        let path = op.owner.join(format!("sync-{}.bak", op.id));
        check_components(&path)?;
        Ok(path)
    }
    fn phase(&self, op: &Operation, phase: &str, error: Option<&str>) -> Result<()> {
        self.db.execute(
            "UPDATE operations SET phase=?,error=? WHERE id=?",
            params![phase, error, op.id.to_string()],
        )?;
        Ok(())
    }
    fn manifest(&self, op: &Operation) -> Result<()> {
        let path = self.home.join(format!("{}.json", op.id));
        write_new(&path, &serde_json::to_vec(op)?)?;
        let read: Operation = serde_json::from_slice(&bounded_read(&path)?)?;
        if serde_json::to_vec(&read)? != serde_json::to_vec(op)? {
            bail!("manifest verification failed");
        }
        Ok(())
    }
    pub fn records(&self) -> Result<Vec<(Operation, String)>> {
        let mut query = self
            .db
            .prepare("SELECT document,phase FROM operations ORDER BY rowid")?;
        let rows = query
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::new();
        for (document, phase) in rows {
            if document.len() > 65536 {
                bail!("oversized recovery journal");
            }
            let op: Operation = serde_json::from_str(&document)?;
            self.target(&op)?;
            let path = self.home.join(format!("{}.json", op.id));
            check_components(&path)?;
            if path.try_exists()? {
                let manifest = bounded_read(&path)?;
                if manifest.len() > 65536
                    || serde_json::to_vec(&serde_json::from_slice::<Operation>(&manifest)?)?
                        != serde_json::to_vec(&op)?
                {
                    bail!("operation manifest does not match journal; review required");
                }
            } else if !["intent", "original_retained"].contains(&phase.as_str()) {
                bail!("operation manifest is missing; review required");
            }
            out.push((op, phase));
        }
        Ok(out)
    }
    pub fn pending(&self) -> Result<bool> {
        Ok(self
            .records()?
            .iter()
            .any(|(_, phase)| !TERMINAL.contains(&phase.as_str())))
    }
    pub fn prepare(
        &self,
        owner: &Path,
        path: &Path,
        original: &[u8],
        output: &[u8],
        kind: &str,
    ) -> Result<Operation> {
        if kind != "restore" && self.pending()? {
            bail!("resolve pending local operations before another write");
        }
        let mut retained = 0u64;
        for (previous, _) in self.records()? {
            let mut paths = vec![
                self.artifact(&previous, "original")?,
                self.artifact(&previous, "candidate")?,
                self.artifact(&previous, "win-backup")?,
                self.artifact(&previous, "recovery")?,
            ];
            if let Ok(private) = self.private(&previous) {
                paths.push(private);
            }
            for path in paths {
                if let Ok(meta) = fs::metadata(path) {
                    retained = retained.saturating_add(meta.len());
                }
            }
        }
        let required = (original.len() as u64)
            .saturating_mul(3)
            .saturating_add(output.len() as u64);
        if retained.saturating_add(required) > 1024 * 1024 * 1024 {
            bail!("recovery storage budget reached; retained evidence was not deleted");
        }
        free_space(&self.root, required.saturating_add(64 * 1024 * 1024))?;
        free_space(owner, required.saturating_add(64 * 1024 * 1024))?;
        let op = Operation {
            id: Uuid::new_v4(),
            installation: self.identity.clone(),
            source: path.canonicalize()?.strip_prefix(&self.root)?.to_path_buf(),
            owner: owner.canonicalize()?,
            original_hash: hash(original),
            output_hash: hash(output),
            original_size: original.len() as u64,
            output_size: output.len() as u64,
            created_at: now(),
            kind: kind.into(),
        };
        self.target(&op)?;
        self.db.execute(
            "INSERT INTO operations VALUES(?,?,'intent',NULL)",
            params![op.id.to_string(), serde_json::to_string(&op)?],
        )?;
        // Record intent and a recoverable target before creating any backup or candidate.
        self.manifest(&op)?;
        write_new(&self.private(&op)?, original)?;
        write_new(&self.artifact(&op, "original")?, original)?;
        write_new(&self.artifact(&op, "candidate")?, output)?;
        self.verify(&op)?;
        self.phase(&op, "prepared", None)?;
        Ok(op)
    }
    fn verify(&self, op: &Operation) -> Result<()> {
        for (path, digest, size) in [
            (self.private(op)?, &op.original_hash, op.original_size),
            (
                self.artifact(op, "original")?,
                &op.original_hash,
                op.original_size,
            ),
            (
                self.artifact(op, "candidate")?,
                &op.output_hash,
                op.output_size,
            ),
        ] {
            let bytes = bounded_read(&path)?;
            if hash(&bytes) != *digest || bytes.len() as u64 != size {
                bail!("recovery artifact verification failed");
            }
        }
        Ok(())
    }
    fn original(&self, op: &Operation) -> Result<Vec<u8>> {
        let mut paths = vec![
            self.artifact(op, "original")?,
            self.artifact(op, "win-backup")?,
        ];
        if let Ok(private) = self.private(op) {
            paths.push(private);
        }
        for path in paths {
            if let Ok(bytes) = bounded_read(&path) {
                if hash(&bytes) == op.original_hash && bytes.len() as u64 == op.original_size {
                    return Ok(bytes);
                }
            }
        }
        bail!("no verified original backup remains; review required")
    }
    fn reconcile(&self, op: &Operation, db: &Connection, digest: &str) -> Result<()> {
        let source = hash(op.source.to_string_lossy().as_bytes());
        db.execute("INSERT INTO sources VALUES(?,?) ON CONFLICT(source) DO UPDATE SET checksum=excluded.checksum",params![source,digest])?;
        Ok(())
    }
    pub fn commit(
        &self,
        op: &Operation,
        db: &Connection,
        guard: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.commit_with(op, db, guard, &replace_with_backup)
    }
    fn commit_with(
        &self,
        op: &Operation,
        db: &Connection,
        guard: &dyn Fn() -> Result<()>,
        replace: &dyn Fn(&Path, &Path, &Path) -> Result<()>,
    ) -> Result<()> {
        guard()?;
        self.verify(op)?;
        let target = self.target(op)?;
        // Keep its directory from being renamed and deny in-place writes to the source.
        // FILE_SHARE_DELETE is required by ReplaceFileW. External renames remain a
        // documented race, bounded by identity/hash checks and post-call inspection.
        let (_parent_handle, _) = root_handle(target.parent().unwrap())?;
        let mut open = fs::OpenOptions::new();
        open.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            open.share_mode(
                windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ
                    | windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE,
            );
        }
        let source_handle = open.open(&target)?;
        let current = bounded_read(&target)?;
        if hash(&current) != op.original_hash {
            self.phase(op, "review_required", Some("source changed after preview"))?;
            bail!("source changed after preview; all files retained");
        }
        guard()?;
        self.phase(op, "replace_intent", None)?;
        let result = replace(
            &target,
            &self.artifact(op, "candidate")?,
            &self.artifact(op, "win-backup")?,
        );
        drop(source_handle);
        // Classify observed files even when Windows reports success or an intermediate failure.
        let outcome = self.recover_one(op, db, guard);
        if let Err(error) = result {
            outcome?;
            return Err(error.context("replacement failed; observed file state was reconciled"));
        }
        outcome?;
        if hash(&bounded_read(&target)?) != op.output_hash {
            bail!("replacement did not publish expected output; original recovered");
        }
        Ok(())
    }
    fn recover_one(
        &self,
        op: &Operation,
        db: &Connection,
        guard: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        let target = self.target(op)?;
        // Never turn access denied or a sharing error into an apparent missing source.
        let state = match bounded_read(&target) {
            Ok(bytes) => Some(hash(&bytes)),
            Err(error) if is_missing(&error) => None,
            Err(error) => return Err(error),
        };
        if state.as_deref() == Some(&op.output_hash) {
            self.original(op)
                .context("output retained but backup health is degraded; next update blocked")?;
            ahdb_format::parse(&bounded_read(&target)?)
                .context("output parser check failed; review required")?;
            self.reconcile(op, db, &op.output_hash)?;
            self.phase(op, "committed", None)?;
            return Ok(());
        }
        if state.as_deref() == Some(&op.original_hash) {
            self.phase(op, "original_retained", None)?;
            return Ok(());
        }
        if state.is_some() {
            self.phase(
                op,
                "review_required",
                Some("unexpected current source; automatic overwrite refused"),
            )?;
            bail!("unexpected source retained; review required");
        }
        let original = self.original(op)?;
        guard()?;
        let (_parent_handle, _) = root_handle(target.parent().unwrap())?;
        let recovery = self.artifact(op, "recovery")?;
        if recovery.exists() {
            if hash(&bounded_read(&recovery)?) != op.original_hash {
                bail!("unexpected recovery candidate retained; review required");
            }
        } else {
            write_new(&recovery, &original)?;
        }
        guard()?;
        self.target(op)?;
        publish_missing(&recovery, &target)?;
        if hash(&bounded_read(&target)?) != op.original_hash {
            bail!("restored source verification failed");
        }
        self.phase(op, "restored", None)?;
        Ok(())
    }
    pub fn recover(&self, db: &Connection, guard: &dyn Fn() -> Result<()>) -> Result<()> {
        guard()?;
        let mut failed = 0;
        for (op, phase) in self.records()? {
            if phase == "committed" && self.original(&op).is_err() {
                failed += 1;
                eprintln!(
                    "operation={} committed output retained but backup health is degraded",
                    op.id
                );
                continue;
            }
            if TERMINAL.contains(&phase.as_str()) {
                continue;
            }
            if let Err(error) = self.recover_one(&op, db, guard) {
                self.phase(&op, "review_required", Some(&format!("{error:#}")))?;
                eprintln!("operation={} recovery deferred: {error:#}", op.id);
                failed += 1;
            }
        }
        if failed > 0 {
            bail!("{failed} operation(s) require recovery or review; game launch is blocked");
        }
        Ok(())
    }
    pub fn status(&self) -> Result<serde_json::Value> {
        let mut rows = vec![];
        for (op, phase) in self.records()? {
            let path = self.target(&op)?;
            let current = match bounded_read(&path) {
                Ok(bytes) => Some(hash(&bytes)),
                Err(error) if is_missing(&error) => None,
                Err(error) => return Err(error),
            };
            rows.push(serde_json::json!({"operation_id":op.id,"account":op.source.components().nth(2).unwrap().as_os_str().to_string_lossy(),"path":path,"created_at":op.created_at,"phase":phase,"kind":op.kind,"original_hash":op.original_hash,"output_hash":op.output_hash,"current_hash":current,"backup_verified":self.original(&op).is_ok()}));
        }
        Ok(serde_json::json!({"installation":self.identity,"operations":rows}))
    }
    pub fn restore(
        &self,
        owner: &Path,
        db: &mut Connection,
        id: Uuid,
        expected: &str,
        guard: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        let op = self
            .records()?
            .into_iter()
            .find(|(op, _)| op.id == id)
            .map(|(op, _)| op)
            .context("unknown operation")?;
        let target = self.target(&op)?;
        let original = self.original(&op)?;
        ahdb_format::parse(&original).context("recorded original is not valid addon data")?;
        guard()?;
        if expected == "missing" {
            if target.try_exists()? {
                bail!("source appeared after preview; restore refused");
            }
            return self.recover_one(&op, db, guard);
        }
        let current = stable_read(&target)?;
        if hash(&current) != expected {
            bail!("current source changed after preview; restore refused");
        }
        if current == original {
            return Ok(());
        }
        let restore = self.prepare(owner, &target, &current, &original, "restore")?;
        // Restoring is an explicit local action. Back up current bytes before any mutation.
        self.commit(&restore, db, guard)?;
        // Resolve this selected operation only after the restore is verified.
        self.phase(&op, "restored", None)?;
        Ok(())
    }
    pub fn capture(&self, db: &Connection, relative: &Path, bytes: &[u8]) -> Result<bool> {
        let digest = hash(bytes);
        for (op, phase) in self.records()? {
            if op.source == relative && !TERMINAL.contains(&phase.as_str()) {
                bail!("source has an unresolved recovery operation; acquisition deferred");
            }
            if op.source == relative && phase == "committed" && op.output_hash == digest {
                self.reconcile(&op, db, &digest)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn is_missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}
#[cfg(windows)]
fn free_space(path: &Path, required: u64) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    if available < required {
        bail!("not enough free space for recovery copies and reserve; source retained");
    }
    Ok(())
}
#[cfg(not(windows))]
fn free_space(_: &Path, _: u64) -> Result<()> {
    bail!("requires Windows");
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    check_components(path)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if bounded_read(path)? != bytes {
        bail!("durable artifact verification failed");
    }
    Ok(())
}
#[cfg(windows)]
pub fn replace_with_backup(source: &Path, temp: &Path, backup: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    for path in [source, temp, backup] {
        check_components(path)?;
    }
    if backup.try_exists()? {
        bail!("replacement backup already exists; source retained");
    }
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let (source, temp, backup) = (wide(source), wide(temp), wide(backup));
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
            source.as_ptr(),
            temp.as_ptr(),
            backup.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn replace_with_backup(_: &Path, _: &Path, _: &Path) -> Result<()> {
    bail!("requires Windows");
}
#[cfg(windows)]
fn publish_missing(temp: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let (temp, target) = (wide(temp), wide(target));
    // No REPLACE_EXISTING: an unexpected source that appears during recovery is preserved.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            temp.as_ptr(),
            target.as_ptr(),
            windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn publish_missing(_: &Path, _: &Path) -> Result<()> {
    bail!("requires Windows");
}

pub fn legacy_pending(db: &Connection) -> Result<()> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='sync_journal')",
        [],
        |r| r.get(0),
    )?;
    if exists {
        let count: i64 = db.query_row("SELECT count(*) FROM sync_journal", [], |r| r.get(0))?;
        if count > 0 {
            bail!("legacy sync journal needs manual account-path verification; retained records were not deleted");
        }
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    const ORIGINAL:&[u8]=br#"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={},daily={},scans={}}}}"#;
    const OUTPUT:&[u8]=br#"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={[1]="100,2,3,4"},daily={},scans={}}}}"#;
    fn fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf, PathBuf, Connection)> {
        let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))?;
        let root = dir.path().join("game");
        let owner = dir.path().join("private");
        let path = root.join("WTF/Account/A/SavedVariables/AuctionDB.lua");
        fs::create_dir_all(path.parent().unwrap())?;
        fs::create_dir_all(&owner)?;
        fs::write(&path, ORIGINAL)?;
        let db = database(&owner)?;
        Ok((dir, root, owner, path, db))
    }
    #[test]
    fn installation_lock_is_shared_across_data_directories() -> Result<()> {
        let (_dir, root, _owner, _path, _db) = fixture()?;
        let first = Coordinator::open(&root, true)?.unwrap();
        assert!(Coordinator::open(&root, true).is_err());
        drop(first);
        assert!(Coordinator::open(&root, true)?.is_some());
        Ok(())
    }
    #[test]
    fn replacement_failures_1175_1176_1177_recover_missing_source_offline() -> Result<()> {
        for code in [1175, 1176, 1177] {
            let (_dir, root, owner, path, db) = fixture()?;
            let c = Coordinator::open(&root, true)?.unwrap();
            let op = c.prepare(&owner, &path, ORIGINAL, OUTPUT, "sync")?;
            let fault = |source: &Path, _temp: &Path, backup: &Path| -> Result<()> {
                if code != 1175 {
                    fs::rename(source, backup)?;
                }
                Err(std::io::Error::from_raw_os_error(code).into())
            };
            assert!(c.commit_with(&op, &db, &|| Ok(()), &fault).is_err());
            assert_eq!(bounded_read(&path)?, ORIGINAL);
            assert!(!c.pending()?);
            assert!(c.artifact(&op, "candidate")?.exists());
        }
        Ok(())
    }
    #[test]
    fn restart_recovers_missing_file_without_pairing_or_discovery() -> Result<()> {
        let (_dir, root, owner, path, db) = fixture()?;
        let c = Coordinator::open(&root, true)?.unwrap();
        let op = c.prepare(&owner, &path, ORIGINAL, OUTPUT, "sync")?;
        fs::rename(&path, c.artifact(&op, "win-backup")?)?;
        drop(c);
        assert!(discover(&root)?.is_empty());
        let c = Coordinator::open(&root, false)?.unwrap();
        c.recover(&db, &|| Ok(()))?;
        assert_eq!(bounded_read(&path)?, ORIGINAL);
        c.recover(&db, &|| Ok(()))?;
        Ok(())
    }
    #[test]
    fn unknown_source_and_corrupt_backups_are_never_overwritten() -> Result<()> {
        let (_dir, root, owner, path, db) = fixture()?;
        let c = Coordinator::open(&root, true)?.unwrap();
        let op = c.prepare(&owner, &path, ORIGINAL, OUTPUT, "sync")?;
        fs::write(&path, b"unexpected game save")?;
        assert!(c.recover(&db, &|| Ok(())).is_err());
        assert_eq!(bounded_read(&path)?, b"unexpected game save");
        fs::remove_file(&path)?;
        fs::write(c.artifact(&op, "original")?, b"corrupt")?;
        fs::write(c.private(&op)?, b"corrupt")?;
        assert!(c.recover(&db, &|| Ok(())).is_err());
        assert!(!path.exists());
        assert!(c.pending()?);
        Ok(())
    }
    #[test]
    fn restore_checks_preview_and_preserves_pre_restore_bytes() -> Result<()> {
        let (_dir, root, owner, path, mut db) = fixture()?;
        let c = Coordinator::open(&root, true)?.unwrap();
        let op = c.prepare(&owner, &path, ORIGINAL, OUTPUT, "sync")?;
        c.commit(&op, &db, &|| Ok(()))?;
        assert_eq!(bounded_read(&path)?, OUTPUT);
        assert!(c
            .restore(&owner, &mut db, op.id, &hash(b"wrong"), &|| Ok(()))
            .is_err());
        assert_eq!(bounded_read(&path)?, OUTPUT);
        c.restore(&owner, &mut db, op.id, &hash(OUTPUT), &|| Ok(()))?;
        assert_eq!(bounded_read(&path)?, ORIGINAL);
        assert!(c
            .records()?
            .iter()
            .any(|(op, _)| op.kind == "restore" && op.original_hash == hash(OUTPUT)));
        Ok(())
    }
    #[test]
    fn process_guard_defers_recovery_and_path_traversal_is_rejected() -> Result<()> {
        let (_dir, root, owner, path, db) = fixture()?;
        let c = Coordinator::open(&root, true)?.unwrap();
        let mut op = c.prepare(&owner, &path, ORIGINAL, OUTPUT, "sync")?;
        fs::remove_file(&path)?;
        assert!(c.recover(&db, &|| bail!("game running")).is_err());
        assert!(!path.exists());
        op.source = PathBuf::from("WTF/Account/../SavedVariables/AuctionDB.lua");
        assert!(c.target(&op).is_err());
        Ok(())
    }
    #[test]
    fn native_changed_file_can_be_restored_only_with_exact_explicit_preview() -> Result<()> {
        let (_dir, root, owner, path, mut db) = fixture()?;
        let c = Coordinator::open(&root, true)?.unwrap();
        let op = c.prepare(&owner, &path, ORIGINAL, OUTPUT, "sync")?;
        fs::write(&path, b"unexpected native save")?;
        assert!(c.recover(&db, &|| Ok(())).is_err());
        c.restore(
            &owner,
            &mut db,
            op.id,
            &hash(b"unexpected native save"),
            &|| Ok(()),
        )?;
        assert_eq!(bounded_read(&path)?, ORIGINAL);
        assert!(!c.pending()?);
        Ok(())
    }
    #[test]
    fn preparation_crash_boundaries_leave_original_and_recover_idempotently() -> Result<()> {
        for boundary in 0..4 {
            let (_dir, root, owner, path, db) = fixture()?;
            let c = Coordinator::open(&root, true)?.unwrap();
            let op = c.prepare(&owner, &path, ORIGINAL, OUTPUT, "sync")?;
            c.phase(&op, "intent", None)?;
            let artifacts = [
                c.private(&op)?,
                c.artifact(&op, "original")?,
                c.artifact(&op, "candidate")?,
            ];
            for artifact in artifacts.iter().skip(boundary) {
                fs::remove_file(artifact)?;
            }
            drop(c);
            let c = Coordinator::open(&root, false)?.unwrap();
            c.recover(&db, &|| Ok(()))?;
            c.recover(&db, &|| Ok(()))?;
            assert_eq!(bounded_read(&path)?, ORIGINAL);
            assert!(!c.pending()?);
        }
        Ok(())
    }
    #[test]
    fn second_spool_recognizes_committed_import_without_uploading_it() -> Result<()> {
        let (dir, root, owner, path, db) = fixture()?;
        let c = Coordinator::open(&root, true)?.unwrap();
        let op = c.prepare(&owner, &path, ORIGINAL, OUTPUT, "sync")?;
        c.commit(&op, &db, &|| Ok(()))?;
        drop(c);
        let mut second = database(&dir.path().join("other-private"))?;
        acquire(&mut second, &root)?;
        assert_eq!(
            second.query_row("SELECT count(*) FROM queue", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            second.query_row("SELECT checksum FROM sources", [], |r| r
                .get::<_, String>(0))?,
            hash(OUTPUT)
        );
        Ok(())
    }
    #[test]
    fn active_writer_defers_capture_without_stopping_upload_processing() -> Result<()> {
        let (_dir, root, _owner, _path, mut db) = fixture()?;
        let _c = Coordinator::open(&root, true)?.unwrap();
        assert!(acquire(&mut db, &root)?);
        assert_eq!(
            db.query_row("SELECT count(*) FROM queue", [], |r| r.get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn lock_child() -> Result<()> {
        if let Some(root) = std::env::var_os("NAUCTIONS_TEST_LOCK_ROOT") {
            assert!(Coordinator::open(Path::new(&root), true).is_err());
        }
        Ok(())
    }
    #[test]
    fn independent_process_cannot_bypass_installation_lock() -> Result<()> {
        let (_dir, root, _owner, _path, _db) = fixture()?;
        let _c = Coordinator::open(&root, true)?.unwrap();
        let child = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "sync_safety::tests::lock_child", "--nocapture"])
            .env("NAUCTIONS_TEST_LOCK_ROOT", &root)
            .output()?;
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        Ok(())
    }
}
