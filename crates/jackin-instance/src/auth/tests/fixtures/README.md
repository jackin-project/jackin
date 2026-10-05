# OMP SQLite fixtures

Regenerate these canary-only fixtures with:

```sh
python3 crates/jackin-instance/src/auth/tests/fixtures/generate_omp_fixtures.py
```

The generator uses Python's standard-library `sqlite3` module to create the
database, committed WAL transactions, and before/after checkpoint images. It
also saves a schema-and-credential WAL-only database and constructs a reused
WAL whose valid new-generation prefix is followed by real old-generation
frames. The big-endian fixture re-encodes a real multi-commit SQLite WAL using
the alternate checksum byte order from SQLite's file format. Fixture values
are synthetic `fixture-*` canaries, never real credentials.
