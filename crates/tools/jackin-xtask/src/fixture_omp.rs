use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};

const FIXTURE_DIRECTORY: &str = "crates/services/jackin-instance-roles/src/tests/fixtures";
const SCHEMA: &str = "CREATE TABLE credentials (provider TEXT, value TEXT, profile TEXT)";
const WAL_HEADER_BYTES: usize = 32;
const WAL_FRAME_HEADER_BYTES: usize = 24;
const WAL_MAGIC_LITTLE_CHECKSUMS: u32 = 0x377F_0682;
const WAL_MAGIC_BIG_CHECKSUMS: u32 = 0x377F_0683;

type Artifacts = BTreeMap<String, Vec<u8>>;

pub(crate) fn run(root: &Path) -> Result<()> {
    let output = root.join(FIXTURE_DIRECTORY);
    fs::create_dir_all(&output)
        .with_context(|| format!("creating OMP fixture directory {}", output.display()))?;
    let temporary = tempfile::Builder::new()
        .prefix("jackin-omp-fixtures-")
        .tempdir()
        .context("creating temporary OMP fixture workspace")?;
    let body_result = (|| {
        let artifacts = generate(temporary.path())?;
        validate_artifacts(&artifacts)?;
        preflight_destinations(&output, &artifacts)?;
        Ok(artifacts)
    })();
    let artifacts = finish_with_cleanup(
        body_result,
        temporary.close(),
        "removing temporary OMP fixture workspace",
    )?;

    for (name, bytes) in &artifacts {
        let destination = output.join(name);
        fs::write(&destination, bytes)
            .with_context(|| format!("writing OMP fixture {}", destination.display()))?;
    }
    writeln!(
        io::stdout().lock(),
        "generated SQLite {} WAL fixtures in {}",
        rusqlite::version(),
        output.display()
    )
    .context("writing OMP fixture command result")?;
    Ok(())
}

fn preflight_destinations(output: &Path, artifacts: &Artifacts) -> Result<()> {
    for name in artifacts.keys() {
        let destination = output.join(name);
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => bail!(
                "refusing to replace non-regular OMP fixture: {}",
                destination.display()
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("checking OMP fixture destination {}", destination.display())
                });
            }
        }
    }
    Ok(())
}

fn finish_with_cleanup<T>(
    body: Result<T>,
    cleanup: io::Result<()>,
    cleanup_context: &'static str,
) -> Result<T> {
    match (body, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error).context(cleanup_context),
        (Err(body_error), Err(cleanup_error)) => {
            Err(body_error).context(format!("{cleanup_context}: {cleanup_error}"))
        }
    }
}

fn close_connection(connection: Connection, context: &'static str) -> Result<()> {
    match connection.close() {
        Ok(()) => Ok(()),
        Err((connection, error)) => {
            drop(connection);
            Err(error).context(context)
        }
    }
}

fn generate(temporary: &Path) -> Result<Artifacts> {
    let mut artifacts = Artifacts::new();
    let (current_database, current_wal) = generate_current_fixtures(temporary, &mut artifacts)?;
    generate_schema_fixture(temporary, &mut artifacts)?;
    generate_reused_wal_fixture(temporary, &mut artifacts)?;
    generate_uncommitted_spill_fixture(temporary, &mut artifacts)?;
    generate_big_endian_fixture(&current_database, &current_wal, &mut artifacts)?;
    verify_generated_pairs(&artifacts)?;
    Ok(artifacts)
}

fn generate_current_fixtures(
    temporary: &Path,
    artifacts: &mut Artifacts,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let database = temporary.join("omp-current.db");
    let connection = connect(&database)?;
    connection
        .execute_batch(SCHEMA)
        .context("creating current fixture schema")?;
    insert_credential(&connection, "fixture-db-stale-token")?;
    checkpoint(&connection, Checkpoint::Truncate)?;
    update_credential(&connection, "fixture-wal-first-commit")?;
    update_credential(&connection, "fixture-wal-current-token")?;
    ensure_credential(&connection, "fixture-wal-current-token")?;
    let (pre_checkpoint_db, pre_checkpoint_wal) = save_pair(&database)?;
    if pre_checkpoint_db
        .windows(b"fixture-wal-current-token".len())
        .any(|window| window == b"fixture-wal-current-token")
    {
        bail!("pre-checkpoint database unexpectedly contains the WAL-only token");
    }
    add_pair(
        artifacts,
        "omp-real-current",
        &pre_checkpoint_db,
        &pre_checkpoint_wal,
    );
    checkpoint(&connection, Checkpoint::Truncate)?;
    let (checkpointed_db, checkpointed_wal) = save_pair(&database)?;
    if !contains(&checkpointed_db, b"fixture-wal-current-token") || !checkpointed_wal.is_empty() {
        bail!("checkpoint fixture does not contain the committed database image");
    }
    add_pair(
        artifacts,
        "omp-real-checkpointed",
        &checkpointed_db,
        &checkpointed_wal,
    );
    close_connection(connection, "closing current fixture database")?;
    Ok((pre_checkpoint_db, pre_checkpoint_wal))
}

fn generate_schema_fixture(temporary: &Path, artifacts: &mut Artifacts) -> Result<()> {
    let schema_database = temporary.join("omp-schema-wal-only.db");
    let schema_connection = connect(&schema_database)?;
    schema_connection
        .execute_batch(SCHEMA)
        .context("creating schema-only WAL table")?;
    insert_credential(&schema_connection, "fixture-schema-wal-token")?;
    ensure_credential(&schema_connection, "fixture-schema-wal-token")?;
    let (schema_db, schema_wal) = save_pair(&schema_database)?;
    if contains(&schema_db, b"credentials")
        || contains(&schema_db, b"fixture-schema-wal-token")
        || !contains(&schema_wal, b"fixture-schema-wal-token")
    {
        bail!("schema-only WAL fixture does not keep its schema and row only in the WAL");
    }
    add_pair(artifacts, "omp-schema-wal-only", &schema_db, &schema_wal);
    close_connection(schema_connection, "closing schema-only fixture database")?;
    Ok(())
}

fn generate_reused_wal_fixture(temporary: &Path, artifacts: &mut Artifacts) -> Result<()> {
    let reused_database = temporary.join("omp-reused.db");
    let reused_connection = connect(&reused_database)?;
    reused_connection
        .execute_batch(SCHEMA)
        .context("creating reused WAL table")?;
    insert_credential(&reused_connection, "fixture-db-reused-base")?;
    checkpoint(&reused_connection, Checkpoint::Truncate)?;
    update_credential(
        &reused_connection,
        &format!("fixture-{}", "x".repeat(16_384)),
    )?;
    let old_generation_wal = read_wal(&reused_database)?;
    checkpoint(&reused_connection, Checkpoint::Full)?;
    update_credential(&reused_connection, "fixture-reused-current-token")?;
    ensure_credential(&reused_connection, "fixture-reused-current-token")?;
    let (reused_db, current_generation_wal) = save_pair(&reused_database)?;
    let current_generation = wal_current_generation(&current_generation_wal)?;
    verify_current_generation_checksums(&current_generation_wal, current_generation)?;
    if current_generation.frames == 0 {
        bail!("SQLite reused WAL has no current-generation frames");
    }
    let old_salts = wal_salts(&old_generation_wal)?;
    if old_salts == current_generation.salts {
        bail!("SQLite did not reset WAL salts between generations");
    }
    let mut stale_suffix_wal = current_generation_wal.clone();
    stale_suffix_wal.extend_from_slice(
        old_generation_wal
            .get(WAL_HEADER_BYTES..)
            .context("old WAL generation has no header")?,
    );
    add_pair(
        artifacts,
        "omp-reused-stale-suffix",
        &reused_db,
        &stale_suffix_wal,
    );
    let stale_generation = wal_current_generation(&stale_suffix_wal)?;
    if stale_generation.frames != current_generation.frames
        || stale_generation.salts != current_generation.salts
    {
        bail!("reused WAL stale suffix changed SQLite's current generation boundary");
    }
    verify_current_generation_checksums(&stale_suffix_wal, stale_generation)?;
    close_connection(reused_connection, "closing reused-WAL fixture database")?;
    Ok(())
}

fn generate_uncommitted_spill_fixture(temporary: &Path, artifacts: &mut Artifacts) -> Result<()> {
    let spill_database = temporary.join("omp-reused-uncommitted.db");
    let spill_connection = connect(&spill_database)?;
    spill_connection
        .execute_batch(
            "CREATE TABLE credentials (provider TEXT PRIMARY KEY, value TEXT, profile TEXT);
             CREATE TABLE filler (id INTEGER PRIMARY KEY, payload TEXT);",
        )
        .context("creating uncommitted spill fixture schema")?;
    insert_credential(&spill_connection, "fixture-db-spill-base-token")?;
    {
        let mut insert = spill_connection
            .prepare("INSERT INTO filler(payload) VALUES (?1)")
            .context("preparing filler rows")?;
        for index in 0..500 {
            insert
                .execute([format!("base-{index}-{}", "b".repeat(400))])
                .with_context(|| format!("inserting filler row {index}"))?;
        }
    }
    checkpoint(&spill_connection, Checkpoint::Truncate)?;
    spill_connection
        .execute_batch(
            "BEGIN IMMEDIATE;
             UPDATE credentials SET value='fixture-old-generation-token' WHERE provider='openai';
             UPDATE filler SET payload='old-generation-' || id || 'x' || substr(payload,1,400);
             COMMIT;
             UPDATE credentials SET value='fixture-main-selected-token' WHERE provider='openai';",
        )
        .context("creating committed old WAL generation")?;
    checkpoint(&spill_connection, Checkpoint::Full)?;
    let old_generation_salts = wal_salts(&read_wal(&spill_database)?)?;
    spill_connection
        .pragma_update(None, "cache_size", 2)
        .context("setting small SQLite cache for spill fixture")?;
    spill_connection
        .execute_batch("BEGIN IMMEDIATE")
        .context("starting uncommitted spill transaction")?;
    spill_connection
        .execute(
            "UPDATE filler SET payload='fixture-uncommitted-marker-' || id || substr(payload,1,400)",
            [],
        )
        .context("spilling uncommitted filler pages into WAL")?;
    let (spill_db, spill_wal) = save_pair(&spill_database)?;
    let current = wal_current_generation(&spill_wal)?;
    verify_current_generation_checksums(&spill_wal, current)?;
    if old_generation_salts == current.salts {
        bail!("SQLite did not reset WAL salts for uncommitted spill fixture");
    }
    if current.frames == 0 || current.frames >= current.total_frames {
        bail!("uncommitted spill fixture has no stale old-generation suffix");
    }
    if current.commits != 0 {
        bail!("uncommitted spill fixture unexpectedly contains a commit");
    }
    if !contains(&spill_db, b"fixture-main-selected-token")
        || !contains(&spill_wal, b"fixture-uncommitted-marker")
    {
        bail!("uncommitted spill fixture lost expected database or WAL content");
    }
    add_pair(
        artifacts,
        "omp-reused-uncommitted-stale-suffix",
        &spill_db,
        &spill_wal,
    );
    spill_connection
        .execute_batch("ROLLBACK")
        .context("rolling back uncommitted spill fixture")?;
    close_connection(
        spill_connection,
        "closing uncommitted-spill fixture database",
    )?;
    Ok(())
}

fn generate_big_endian_fixture(
    database: &[u8],
    wal: &[u8],
    artifacts: &mut Artifacts,
) -> Result<()> {
    if read_u32_be(wal, 0)? != WAL_MAGIC_LITTLE_CHECKSUMS {
        bail!("expected bundled SQLite to emit little-endian WAL checksums");
    }
    let big_endian_wal = big_endian_checksum_fixture(wal)?;
    verify_wal_checksums(wal)?;
    verify_wal_checksums(&big_endian_wal)?;
    add_pair(
        artifacts,
        "omp-real-current-big-endian",
        database,
        &big_endian_wal,
    );
    Ok(())
}

fn verify_generated_pairs(artifacts: &Artifacts) -> Result<()> {
    for (stem, expected) in [
        ("omp-real-current", "fixture-wal-current-token"),
        ("omp-real-checkpointed", "fixture-wal-current-token"),
        ("omp-schema-wal-only", "fixture-schema-wal-token"),
        ("omp-reused-stale-suffix", "fixture-reused-current-token"),
        (
            "omp-reused-uncommitted-stale-suffix",
            "fixture-main-selected-token",
        ),
        ("omp-real-current-big-endian", "fixture-wal-current-token"),
    ] {
        verify_pair(
            artifacts
                .get(&format!("{stem}.db"))
                .context("generated database artifact is missing")?,
            artifacts
                .get(&format!("{stem}.db-wal"))
                .context("generated WAL artifact is missing")?,
            expected,
        )
        .with_context(|| format!("validating generated SQLite pair {stem}"))?;
    }
    Ok(())
}

fn connect(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path)
        .with_context(|| format!("opening SQLite fixture database {}", path.display()))?;
    connection
        .pragma_update(None, "page_size", 512)
        .context("setting SQLite fixture page size")?;
    let mode: String = connection
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .context("enabling SQLite WAL mode")?;
    if !mode.eq_ignore_ascii_case("wal") {
        bail!("SQLite did not enable WAL mode: {mode}");
    }
    connection
        .pragma_update(None, "wal_autocheckpoint", 0)
        .context("disabling automatic WAL checkpoints")?;
    Ok(connection)
}

fn checkpoint(connection: &Connection, mode: Checkpoint) -> Result<()> {
    let pragma = match mode {
        Checkpoint::Full => "PRAGMA wal_checkpoint(FULL)",
        Checkpoint::Truncate => "PRAGMA wal_checkpoint(TRUNCATE)",
    };
    let (busy, _log_frames, _checkpointed_frames): (i64, i64, i64) = connection
        .query_row(pragma, [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .with_context(|| format!("running SQLite checkpoint {pragma}"))?;
    if busy != 0 {
        bail!("SQLite checkpoint failed: {pragma} returned busy={busy}");
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Checkpoint {
    Full,
    Truncate,
}

fn insert_credential(connection: &Connection, value: &str) -> Result<()> {
    connection
        .execute(
            "INSERT INTO credentials VALUES (?1, ?2, ?3)",
            params!["openai", value, "work"],
        )
        .context("inserting SQLite fixture credential")?;
    Ok(())
}

fn update_credential(connection: &Connection, value: &str) -> Result<()> {
    connection
        .execute(
            "UPDATE credentials SET value=?1 WHERE provider=?2",
            params![value, "openai"],
        )
        .context("updating SQLite fixture credential")?;
    Ok(())
}

fn ensure_credential(connection: &Connection, expected: &str) -> Result<()> {
    let actual: String = connection
        .query_row("SELECT value FROM credentials", [], |row| row.get(0))
        .context("reading SQLite fixture credential")?;
    if actual != expected {
        bail!("SQLite WAL fixture retained {actual:?}, expected {expected:?}");
    }
    Ok(())
}

fn save_pair(database: &Path) -> Result<(Vec<u8>, Vec<u8>)> {
    let db = fs::read(database)
        .with_context(|| format!("reading SQLite fixture database {}", database.display()))?;
    let wal_path = wal_path(database)?;
    let wal = match fs::read(&wal_path) {
        Ok(wal) => wal,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading SQLite fixture WAL {}", wal_path.display()));
        }
    };
    Ok((db, wal))
}

fn read_wal(database: &Path) -> Result<Vec<u8>> {
    fs::read(wal_path(database)?).with_context(|| {
        format!(
            "reading WAL for SQLite fixture database {}",
            database.display()
        )
    })
}

fn wal_path(database: &Path) -> Result<PathBuf> {
    let mut path = OsString::from(database.as_os_str());
    path.push("-wal");
    Ok(PathBuf::from(path))
}

fn add_pair(artifacts: &mut Artifacts, stem: &str, database: &[u8], wal: &[u8]) {
    artifacts.insert(format!("{stem}.db"), database.to_vec());
    artifacts.insert(format!("{stem}.db-wal"), wal.to_vec());
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn verify_pair(database: &[u8], wal: &[u8], expected: &str) -> Result<()> {
    let temporary = tempfile::Builder::new()
        .prefix("jackin-omp-verify-")
        .tempdir()
        .context("creating temporary SQLite verification directory")?;
    let body_result = (|| {
        let path = temporary.path().join("agent.db");
        fs::write(&path, database).context("writing temporary SQLite verification database")?;
        fs::write(wal_path(&path)?, wal).context("writing temporary SQLite verification WAL")?;
        let connection =
            Connection::open(&path).context("opening temporary SQLite verification pair")?;
        let actual: String = connection
            .query_row("SELECT value FROM credentials", [], |row| row.get(0))
            .context("reading credential from temporary SQLite verification pair")?;
        close_connection(
            connection,
            "closing temporary SQLite verification connection",
        )?;
        if actual != expected {
            bail!("SQLite read {actual:?}, expected {expected:?}");
        }
        Ok(())
    })();
    finish_with_cleanup(
        body_result,
        temporary.close(),
        "removing temporary SQLite verification directory",
    )
}

fn wal_salts(wal: &[u8]) -> Result<(u32, u32)> {
    if wal.len() < WAL_HEADER_BYTES {
        bail!("SQLite WAL is shorter than its header");
    }
    Ok((read_u32_be(wal, 16)?, read_u32_be(wal, 20)?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WalGeneration {
    salts: (u32, u32),
    frames: usize,
    commits: usize,
    total_frames: usize,
}

fn wal_current_generation(wal: &[u8]) -> Result<WalGeneration> {
    if wal.len() < WAL_HEADER_BYTES {
        bail!("SQLite WAL is shorter than its header");
    }
    let page_size =
        usize::try_from(read_u32_be(wal, 8)?).context("converting SQLite WAL page size")?;
    let frame_size = page_size
        .checked_add(WAL_FRAME_HEADER_BYTES)
        .context("computing SQLite WAL frame size")?;
    let body_len = wal.len() - WAL_HEADER_BYTES;
    if frame_size == 0 || !body_len.is_multiple_of(frame_size) {
        bail!("SQLite WAL has a partial frame");
    }
    let salts = wal_salts(wal)?;
    let total_frames = body_len / frame_size;
    let mut frames = 0;
    let mut commits = 0;
    for index in 0..total_frames {
        let start = WAL_HEADER_BYTES + index * frame_size;
        if (read_u32_be(wal, start + 8)?, read_u32_be(wal, start + 12)?) != salts {
            break;
        }
        frames += 1;
        if read_u32_be(wal, start + 4)? > 0 {
            commits += 1;
        }
    }
    Ok(WalGeneration {
        salts,
        frames,
        commits,
        total_frames,
    })
}

fn verify_current_generation_checksums(wal: &[u8], generation: WalGeneration) -> Result<()> {
    let page_size =
        usize::try_from(read_u32_be(wal, 8)?).context("converting SQLite WAL page size")?;
    let frame_size = page_size
        .checked_add(WAL_FRAME_HEADER_BYTES)
        .context("computing SQLite WAL frame size")?;
    let current_len = WAL_HEADER_BYTES
        .checked_add(
            generation
                .frames
                .checked_mul(frame_size)
                .context("computing current SQLite WAL generation length")?,
        )
        .context("computing current SQLite WAL generation length")?;
    let current = wal
        .get(..current_len)
        .context("current SQLite WAL generation is truncated")?;
    verify_wal_checksums(current)
}

fn big_endian_checksum_fixture(wal: &[u8]) -> Result<Vec<u8>> {
    let page_size =
        usize::try_from(read_u32_be(wal, 8)?).context("converting SQLite WAL page size")?;
    let frame_size = page_size
        .checked_add(WAL_FRAME_HEADER_BYTES)
        .context("computing SQLite WAL frame size")?;
    if wal.len() < WAL_HEADER_BYTES || frame_size == 0 {
        bail!("SQLite WAL is shorter than a valid header");
    }
    let body_len = wal.len() - WAL_HEADER_BYTES;
    if !body_len.is_multiple_of(frame_size) {
        bail!("SQLite WAL has a partial frame");
    }

    let mut output = wal.to_vec();
    output[..4].copy_from_slice(&WAL_MAGIC_BIG_CHECKSUMS.to_be_bytes());
    let mut state = sqlite_checksum(&output[..24], (0, 0), ByteOrder::Big)?;
    write_checksum(&mut output, 24, state)?;
    let frame_count = body_len / frame_size;
    for index in 0..frame_count {
        let frame_start = WAL_HEADER_BYTES + index * frame_size;
        let header_end = frame_start + 8;
        state = sqlite_checksum(&output[frame_start..header_end], state, ByteOrder::Big)?;
        let page_start = frame_start + WAL_FRAME_HEADER_BYTES;
        let page_end = page_start + page_size;
        state = sqlite_checksum(&output[page_start..page_end], state, ByteOrder::Big)?;
        write_checksum(&mut output, frame_start + 16, state)?;
    }
    Ok(output)
}

fn verify_wal_checksums(wal: &[u8]) -> Result<()> {
    if wal.len() < WAL_HEADER_BYTES {
        bail!("SQLite WAL is shorter than its header");
    }
    let byte_order = match read_u32_be(wal, 0)? {
        WAL_MAGIC_LITTLE_CHECKSUMS => ByteOrder::Little,
        WAL_MAGIC_BIG_CHECKSUMS => ByteOrder::Big,
        _ => bail!("SQLite WAL has an invalid checksum byte-order marker"),
    };
    let page_size =
        usize::try_from(read_u32_be(wal, 8)?).context("converting SQLite WAL page size")?;
    let frame_size = page_size
        .checked_add(WAL_FRAME_HEADER_BYTES)
        .context("computing SQLite WAL frame size")?;
    let body_len = wal.len() - WAL_HEADER_BYTES;
    if frame_size == 0 || !body_len.is_multiple_of(frame_size) {
        bail!("SQLite WAL has a partial frame");
    }
    let mut state = sqlite_checksum(&wal[..24], (0, 0), byte_order)?;
    if read_u32_be(wal, 24)? != state.0 || read_u32_be(wal, 28)? != state.1 {
        bail!("SQLite WAL header checksum is invalid");
    }
    for index in 0..(body_len / frame_size) {
        let frame_start = WAL_HEADER_BYTES + index * frame_size;
        state = sqlite_checksum(&wal[frame_start..frame_start + 8], state, byte_order)?;
        let page_start = frame_start + WAL_FRAME_HEADER_BYTES;
        state = sqlite_checksum(&wal[page_start..page_start + page_size], state, byte_order)?;
        if read_u32_be(wal, frame_start + 16)? != state.0
            || read_u32_be(wal, frame_start + 20)? != state.1
        {
            bail!("SQLite WAL frame {index} checksum is invalid");
        }
    }
    Ok(())
}

fn write_checksum(bytes: &mut [u8], offset: usize, checksum: (u32, u32)) -> Result<()> {
    let first = bytes
        .get_mut(offset..offset + 4)
        .context("SQLite WAL checksum field is truncated")?;
    first.copy_from_slice(&checksum.0.to_be_bytes());
    let second = bytes
        .get_mut(offset + 4..offset + 8)
        .context("SQLite WAL checksum field is truncated")?;
    second.copy_from_slice(&checksum.1.to_be_bytes());
    Ok(())
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .context("SQLite WAL integer field is truncated")?;
    Ok(u32::from_be_bytes(
        value
            .try_into()
            .context("SQLite WAL integer field has invalid width")?,
    ))
}

#[derive(Clone, Copy)]
enum ByteOrder {
    Little,
    Big,
}

fn sqlite_checksum(
    data: &[u8],
    mut state: (u32, u32),
    byte_order: ByteOrder,
) -> Result<(u32, u32)> {
    if !data.len().is_multiple_of(8) {
        bail!("SQLite checksum input must be 8-byte aligned");
    }
    for words in data.as_chunks::<8>().0 {
        let first: [u8; 4] = words[..4]
            .try_into()
            .context("reading first SQLite checksum word")?;
        let second: [u8; 4] = words[4..]
            .try_into()
            .context("reading second SQLite checksum word")?;
        let (first, second) = match byte_order {
            ByteOrder::Little => (u32::from_le_bytes(first), u32::from_le_bytes(second)),
            ByteOrder::Big => (u32::from_be_bytes(first), u32::from_be_bytes(second)),
        };
        state.0 = state.0.wrapping_add(first).wrapping_add(state.1);
        state.1 = state.1.wrapping_add(second).wrapping_add(state.0);
    }
    Ok(state)
}

fn validate_artifacts(artifacts: &Artifacts) -> Result<()> {
    let expected = [
        "omp-real-current.db",
        "omp-real-current.db-wal",
        "omp-real-checkpointed.db",
        "omp-real-checkpointed.db-wal",
        "omp-schema-wal-only.db",
        "omp-schema-wal-only.db-wal",
        "omp-reused-stale-suffix.db",
        "omp-reused-stale-suffix.db-wal",
        "omp-reused-uncommitted-stale-suffix.db",
        "omp-reused-uncommitted-stale-suffix.db-wal",
        "omp-real-current-big-endian.db",
        "omp-real-current-big-endian.db-wal",
    ];
    if artifacts.len() != expected.len()
        || expected.iter().any(|name| !artifacts.contains_key(*name))
    {
        bail!("generated OMP fixture set does not match the reviewed 12-file inventory");
    }
    for (name, bytes) in artifacts {
        if bytes.is_empty() && name != "omp-real-checkpointed.db-wal" {
            bail!("generated OMP fixture is unexpectedly empty: {name}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
