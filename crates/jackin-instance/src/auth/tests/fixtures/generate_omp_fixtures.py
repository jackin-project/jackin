#!/usr/bin/env python3
"""Generate canary-only SQLite WAL fixtures with the system sqlite3 module."""

from pathlib import Path
import sqlite3
import struct
import tempfile


OUT = Path(__file__).parent
SCHEMA = "CREATE TABLE credentials (provider TEXT, value TEXT, profile TEXT)"


def connect(path: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(path, isolation_level=None)
    connection.execute("PRAGMA page_size=512")
    if connection.execute("PRAGMA journal_mode=WAL").fetchone()[0].lower() != "wal":
        raise RuntimeError("system SQLite did not enable WAL mode")
    connection.execute("PRAGMA wal_autocheckpoint=0")
    return connection


def checkpoint(connection: sqlite3.Connection, mode: str) -> None:
    result = connection.execute(f"PRAGMA wal_checkpoint({mode})").fetchone()
    if result[0] != 0:
        raise RuntimeError(f"SQLite {mode} checkpoint failed: {result}")


def save_pair(database: Path, stem: str) -> tuple[bytes, bytes]:
    db = database.read_bytes()
    wal_path = Path(f"{database}-wal")
    wal = wal_path.read_bytes() if wal_path.exists() else b""
    (OUT / f"{stem}.db").write_bytes(db)
    (OUT / f"{stem}.db-wal").write_bytes(wal)
    return db, wal


def sqlite_checksum(data: bytes, seed: tuple[int, int], byteorder: str) -> tuple[int, int]:
    if len(data) % 8:
        raise ValueError("SQLite checksum input must be 8-byte aligned")
    s0, s1 = seed
    for offset in range(0, len(data), 8):
        x0 = int.from_bytes(data[offset : offset + 4], byteorder)
        x1 = int.from_bytes(data[offset + 4 : offset + 8], byteorder)
        s0 = (s0 + x0 + s1) & 0xFFFFFFFF
        s1 = (s1 + x1 + s0) & 0xFFFFFFFF
    return s0, s1


def big_endian_checksum_fixture(wal: bytes) -> bytes:
    """Re-encode a real SQLite WAL using the other format-defined byte order."""
    output = bytearray(wal)
    page_size = struct.unpack_from(">I", output, 8)[0]
    output[:4] = struct.pack(">I", 0x377F0683)
    state = sqlite_checksum(output[:24], (0, 0), "big")
    struct.pack_into(">II", output, 24, *state)
    frame_size = page_size + 24
    complete_frames = (len(output) - 32) // frame_size
    for index in range(complete_frames):
        frame_at = 32 + index * frame_size
        state = sqlite_checksum(output[frame_at : frame_at + 8], state, "big")
        state = sqlite_checksum(
            output[frame_at + 24 : frame_at + 24 + page_size], state, "big"
        )
        struct.pack_into(">II", output, frame_at + 16, *state)
    return bytes(output)


def verify_pair(database: bytes, wal: bytes, expected: str) -> None:
    """Ask system SQLite to read a copied pair without changing the fixture."""
    with tempfile.TemporaryDirectory(prefix="jackin-omp-verify-") as temp:
        path = Path(temp) / "agent.db"
        path.write_bytes(database)
        Path(f"{path}-wal").write_bytes(wal)
        connection = sqlite3.connect(path)
        actual = connection.execute("SELECT value FROM credentials").fetchone()[0]
        connection.close()
        if actual != expected:
            raise RuntimeError(f"SQLite read {actual!r}, expected {expected!r}")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="jackin-omp-fixtures-") as temp:
        root = Path(temp)

        # Main-file stale credential plus two actual SQLite commits retained
        # in the WAL. Capture both sides of a real TRUNCATE checkpoint for the
        # deterministic checkpoint/reset interleaving test.
        database = root / "omp-current.db"
        connection = connect(database)
        connection.execute(SCHEMA)
        connection.execute(
            "INSERT INTO credentials VALUES (?, ?, ?)",
            ("openai", "fixture-db-stale-token", "work"),
        )
        connection.commit()
        checkpoint(connection, "TRUNCATE")
        connection.execute(
            "UPDATE credentials SET value=? WHERE provider=?",
            ("fixture-wal-first-commit", "openai"),
        )
        connection.execute(
            "UPDATE credentials SET value=? WHERE provider=?",
            ("fixture-wal-current-token", "openai"),
        )
        if connection.execute("SELECT value FROM credentials").fetchone()[0] != "fixture-wal-current-token":
            raise RuntimeError("real SQLite WAL fixture did not retain the latest commit")
        pre_checkpoint_db, pre_checkpoint_wal = save_pair(database, "omp-real-current")
        if b"fixture-wal-current-token" in pre_checkpoint_db:
            raise RuntimeError("pre-checkpoint database unexpectedly contains the WAL-only token")
        checkpoint(connection, "TRUNCATE")
        checkpointed_db, checkpointed_wal = save_pair(database, "omp-real-checkpointed")
        if b"fixture-wal-current-token" not in checkpointed_db or checkpointed_wal:
            raise RuntimeError("checkpoint fixture does not contain the committed database image")
        connection.close()

        # New table and row remain entirely in a real WAL, including the
        # sqlite_schema/page-one update.
        schema_database = root / "omp-schema-wal-only.db"
        schema_connection = connect(schema_database)
        schema_connection.execute(SCHEMA)
        schema_connection.execute(
            "INSERT INTO credentials VALUES (?, ?, ?)",
            ("openai", "fixture-schema-wal-token", "work"),
        )
        if schema_connection.execute("SELECT value FROM credentials").fetchone()[0] != "fixture-schema-wal-token":
            raise RuntimeError("schema-only WAL fixture did not retain its credential")
        schema_db, schema_wal = save_pair(schema_database, "omp-schema-wal-only")
        if b"credentials" in schema_db or b"fixture-schema-wal-token" in schema_db:
            raise RuntimeError("schema fixture unexpectedly wrote schema or row to the main database")
        if b"fixture-schema-wal-token" not in schema_wal:
            raise RuntimeError("schema-only WAL fixture is missing its credential")
        schema_connection.close()

        # Force a long old generation, checkpoint it, and commit a shorter
        # current generation. Append the old frames after the current commit
        # to model a SQLite WAL reset that retains stale trailing frames.
        reused_database = root / "omp-reused.db"
        reused_connection = connect(reused_database)
        reused_connection.execute(SCHEMA)
        reused_connection.execute(
            "INSERT INTO credentials VALUES (?, ?, ?)",
            ("openai", "fixture-db-reused-base", "work"),
        )
        reused_connection.commit()
        checkpoint(reused_connection, "TRUNCATE")
        reused_connection.execute(
            "UPDATE credentials SET value=? WHERE provider=?",
            ("fixture-" + "x" * 16384, "openai"),
        )
        old_generation_wal = Path(f"{reused_database}-wal").read_bytes()
        checkpoint(reused_connection, "FULL")
        reused_connection.execute(
            "UPDATE credentials SET value=? WHERE provider=?",
            ("fixture-reused-current-token", "openai"),
        )
        if reused_connection.execute("SELECT value FROM credentials").fetchone()[0] != "fixture-reused-current-token":
            raise RuntimeError("reused WAL fixture did not retain the current generation")
        reused_db, current_generation_wal = save_pair(
            reused_database, "omp-reused-stale-suffix"
        )
        old_salts = struct.unpack_from(">II", old_generation_wal, 16)
        current_salts = struct.unpack_from(">II", current_generation_wal, 16)
        if old_salts == current_salts:
            raise RuntimeError("SQLite did not reset WAL salts between generations")
        stale_suffix = old_generation_wal[32:]
        (OUT / "omp-reused-stale-suffix.db").write_bytes(reused_db)
        (OUT / "omp-reused-stale-suffix.db-wal").write_bytes(
            current_generation_wal + stale_suffix
        )
        reused_connection.close()

    # The host SQLite emits 0x377f0682 on little-endian systems. Re-encode the
    # actual multi-commit WAL with the separately specified 0x377f0683 byte
    # order so both checksum branches use real SQLite page/frame content.
    if struct.unpack_from(">I", pre_checkpoint_wal, 0)[0] != 0x377F0682:
        raise RuntimeError("expected system SQLite to emit little-endian WAL checksums")
    (OUT / "omp-real-current-big-endian.db").write_bytes(pre_checkpoint_db)
    (OUT / "omp-real-current-big-endian.db-wal").write_bytes(
        big_endian_checksum_fixture(pre_checkpoint_wal)
    )
    for stem, expected in [
        ("omp-real-current", "fixture-wal-current-token"),
        ("omp-real-checkpointed", "fixture-wal-current-token"),
        ("omp-schema-wal-only", "fixture-schema-wal-token"),
        ("omp-reused-stale-suffix", "fixture-reused-current-token"),
        ("omp-real-current-big-endian", "fixture-wal-current-token"),
    ]:
        verify_pair(
            (OUT / f"{stem}.db").read_bytes(),
            (OUT / f"{stem}.db-wal").read_bytes(),
            expected,
        )
    print(f"generated SQLite {sqlite3.sqlite_version} WAL fixtures in {OUT}")


if __name__ == "__main__":
    main()
