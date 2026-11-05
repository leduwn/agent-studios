# M15-04.1 SQLite Durability Tiers & Cross-Process Locking Architecture

## 1. Durability Guarantees & Trade-Offs

SQLite's durability behavior depends fundamentally on filesystem synchronization primitives. In M15-04.1, we explicitly distinguish three failure models:

| Durability Tier | Configuration | Process Crash | OS Crash / Panic | Unexpected Power Loss | Throughput Cost |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Tier 1: High-Performance** | `PRAGMA synchronous = NORMAL;`<br>`PRAGMA journal_mode = WAL;` | **100% Durable** (WAL append atomic) | **Partial Risk** (OS cache uncommitted) | **Risk of WAL truncation** | Low (few fsync per transaction) |
| **Tier 2: Production Acknowledged** *(Default)* | `PRAGMA synchronous = FULL;`<br>`PRAGMA journal_mode = WAL;` | **100% Durable** | **100% Durable** | **100% Durable** (fsync barrier acknowledged) | Moderate (fsync before commit ack) |

### Durability Policy
- **Default Production Mode:** `DurabilityMode::Full` (`PRAGMA synchronous = FULL;`). Guarantees that every event envelope batch acknowledged to the caller has been committed to non-volatile storage via a verified fsync barrier.
- **Configurable Mode:** Operators may set `--durability normal` for high-throughput testing or dev environments.
- **Injection Safety:** The `durability` parameter is mapped to a strongly typed Rust enum (`DurabilityMode`). Arbitrary SQL injection via PRAGMA statements is strictly prohibited.

---

## 2. Cross-Process Single-Writer Database Ownership

### The Defect in Prior State
Prior to M15-04.1, process concurrency relied purely on in-memory `Mutex<Connection>` and SQLite's internal `busy_timeout = 5000`. If a user or script launched two `agsd` daemons targeting the same database path (`%APPDATA%\AgentStudios\agsd.db`), both processes competed for SQLite locks, causing transaction timeouts and potential split-brain recovery conflicts.

### The M15-04.1 Cross-Process Lock Architecture
The `DatabaseLock` primitive guarantees single-writer ownership across distinct OS processes:

1. **Lockfile Placement & Containment:**
   - Database lockfile is created at `<db_path>.lock`.
   - The path is validated with `dunce::canonicalize` on its parent directory, preventing symlink hijacking or directory traversal escapes.
2. **OS-Level Kernel Locking:**
   - **Windows:** Uses `CreateFileW` with `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`, followed by `LockFileEx` with `LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY`.
   - **Unix:** Opens file with `0o600` permissions and issues non-blocking `libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB)`.
3. **Stale Lockfile vs Actively Held Lock:**
   - If an `agsd.db.lock` file exists on disk from an ungraceful crash, but no OS process holds an open file handle with the kernel lock, the lock acquisition **succeeds immediately**.
   - If a living `agsd` process actively holds the kernel lock, acquisition immediately fails with a descriptive error (`StorageFailure("Another agsd process actively holds the database lock")`).
4. **Lifetime & RAII Cleanup:**
   - The `DatabaseLock` instance is held by `IpcServer` throughout its lifetime.
   - On normal shutdown or process death, the operating system kernel automatically drops all file locks.
