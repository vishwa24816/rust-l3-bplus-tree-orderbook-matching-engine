# EncFS - Encrypted Virtual Loopback Filesystem

A local encrypted virtual filesystem implemented in Rust using FUSE (`fuser`), featuring authenticated chunked encryption with ChaCha20-Poly1305 AEAD and hierarchical key derivation via Argon2id + HKDF-SHA256. The project includes a native GUI built with `eframe`/`egui` for easy management of encrypted disk images.

---

## Table of Contents

1. [System Design](#system-design)
2. [Architecture](#architecture)
3. [Implementation Details](#implementation-details)
4. [Building](#building)
5. [Usage Instructions](#usage-instructions)
6. [Security Considerations](#security-considerations)
7. [Project Structure](#project-structure)

---

## System Design

### Overview

EncFS creates an encrypted container file (disk image) that acts as a virtual filesystem. All file data and metadata stored within the container are encrypted using modern cryptographic primitives. The system intercepts filesystem operations, encrypts/decrypts data in 4 KiB chunks, and manages keys hierarchically.

### Threat Model

- **Protected:** Data at rest on the host disk is fully encrypted.
- **Protected:** Master keys and per-file keys are never written to disk in plaintext.
- **Protected:** AEAD authentication ensures any tampering with ciphertext is detected.
- **Not Protected:** Data is exposed in memory while the application is running and the image is mounted/open.

### Design Goals

1. **Zero-Plaintext Persistence:** No unencrypted file chunks or raw master keys touch the host disk.
2. **Authenticated Encryption:** Every 4 KiB block is encrypted with ChaCha20-Poly1305, providing confidentiality and integrity.
3. **Nonce Safety:** Every AEAD block receives a unique, non-reusable nonce.
4. **Read-Modify-Write Safety:** Partial block writes are handled by reading, decrypting, modifying, and re-encrypting the affected chunk.
5. **Error Boundaries:** AEAD tag validation failures return `EIO` (Input/Output Error) without panicking.

---

## Architecture

### Key Derivation Pipeline

```
User Password
     │
     ▼
  Argon2id (Memory-Hard KDF)
     │
     ▼
  Master Key (32 bytes)
     │
     ▼
  HKDF-SHA256 (per-file)
     │
     ▼
  Per-File Key (32 bytes)
```

- **Argon2id:** Used to derive the master key from the user's password. Parameters: `m_cost=65536` (64 MiB), `t_cost=3`, `p_cost=4`.
- **HKDF-SHA256:** Derives a unique per-file key from the master key and the file's UUID. This ensures that compromise of one file key does not reveal others.

### Nonce Derivation

Nonces are derived deterministically using HKDF-SHA256:

```
Nonce = HKDF-SHA256(FileKey, FileUUID || BlockIndex || NonceSequence)
```

- **FileUUID:** Unique identifier for each file.
- **BlockIndex:** 0-based index of the 4 KiB block within the file.
- **NonceSequence:** Monotonic counter per file, incremented after each block allocation.

This ensures every nonce is unique and never reused for the same key.

### Block Layout

Each 4 KiB on-disk block contains:

| Offset | Size | Description |
|--------|------|-------------|
| 0 | 4080 | Encrypted plaintext (ChaCha20-Poly1305 ciphertext) |
| 4080 | 16 | AEAD authentication tag |

The plaintext block size is 4080 bytes; the remaining 16 bytes are the Poly1305 tag.

### Storage Layout

The encrypted image file is structured as follows:

| Offset | Size | Description |
|--------|------|-------------|
| 0 | 4096 | Superblock (salt, version, metadata offset, BAT offset) |
| 4096 | 4096 | Block Allocation Table (BAT) |
| 8192 | N × 4096 | Data blocks (encrypted file contents) |

### In-Memory Data Structures

- **Inode Table:** Maps VFS inode numbers to `InodeEntry` structs containing file metadata (type, size, parent, UUID, next nonce sequence).
- **Block Map:** Maps `(FileUUID, BlockIndex)` to `BlockMapEntry` structs containing the on-disk block offset and nonce sequence.
- **Block Allocator:** First-fit allocator tracking free/used blocks via the BAT.

---

## Implementation Details

### Cryptographic Module (`src/crypto/`)

#### `kdf.rs`

- `derive_master_key(password, salt) -> [u8; 32]`
  - Derives a 32-byte master key from a password and salt using Argon2id.
- `derive_file_key(master_key, file_uuid) -> [u8; 32]`
  - Derives a per-file key using HKDF-SHA256 with the master key and file UUID as info.
- `derive_nonce(file_key, file_uuid, block_index, nonce_sequence) -> [u8; 12]`
  - Derives a 96-bit nonce for a specific block using HKDF-SHA256.

#### `aead.rs`

- `encrypt_chunk(file_key, file_uuid, block_index, nonce_sequence, plaintext) -> [u8; 4096]`
  - Encrypts a 4080-byte plaintext block with ChaCha20-Poly1305.
  - Returns a 4096-byte ciphertext block (4080 bytes ciphertext + 16 bytes tag).
- `decrypt_chunk(file_key, file_uuid, block_index, nonce_sequence, ciphertext) -> [u8; 4080]`
  - Decrypts a 4096-byte ciphertext block.
  - Returns `EncfsError::Io` (mapped to POSIX `EIO`) if AEAD tag verification fails.

### Filesystem Core (`src/fs/`)

#### `image.rs`

- `ImageFile::open(path, read_only) -> Result<Self>`
  - Opens an existing encrypted image file.
- `ImageFile::create(path, size, salt) -> Result<Self>`
  - Creates a new encrypted image file with the given size and salt.
- `read_block(offset, buffer)` / `write_block(offset, buffer)`
  - Reads/writes 4096-byte blocks at the given image offset.

#### `allocator.rs`

- `BlockAllocator::new(total_blocks) -> Self`
  - Initializes the allocator with all blocks marked as free.
- `allocate() -> Result<u32>`
  - Finds the first free block and marks it as used.
- `free(block_num)`
  - Marks a block as free.
- Serialization/deserialization of the BAT to/from the image.

#### `metadata.rs`

- `MetadataStore::new() -> Self`
  - Creates an empty metadata store.
- `create_file(parent, name, file_type) -> Result<u64>`
  - Creates a new file or directory entry and returns its inode.
- `get_inode(inode) -> Option<&InodeEntry>`
- `get_inode_mut(inode) -> Option<&mut InodeEntry>`
- `list_dir(inode) -> Vec<(String, u64, FileType)>`
- `add_block_map(file_uuid, block_index, entry)`
- `get_block_map(file_uuid, block_index) -> Option<&BlockMapEntry>`
- `delete(parent, name) -> Result<()>`
- Serialization/deserialization of the metadata store to/from the image.

#### `passthrough.rs` (FUSE bindings, optional)

Implements `fuser::Filesystem` with handlers for:
- `lookup`, `getattr`, `readdir`
- `mkdir`, `create`, `open`, `read`, `write`, `release`

This module is gated behind the `fuse` feature flag and is primarily intended for Linux/macOS.

### GUI Application (`src/main.rs`)

The GUI is built with `eframe`/`egui` and provides four screens:

1. **Welcome Screen:** Choose to create a new encrypted image or open an existing one.
2. **Create Screen:** Enter image path, size (MB), and password (with confirmation) to create a new encrypted image.
3. **Open Screen:** Enter image path and password to open an existing encrypted image.
4. **Browser Screen:** Navigate directories, view file contents, edit and save files.

The GUI uses `EncryptedFs` as a backend, which wraps the cryptographic and filesystem core in a thread-safe (`Arc<RwLock<...>>`) interface.

---

## Building

### Prerequisites

- **Rust** (latest stable edition, 2021)
- **Cargo** (included with Rust)
- **Windows:** MinGW-w64 toolchain (for linking eframe/egui)
- **Linux/macOS:** FUSE development libraries (only if building with `--features fuse`)

### Build Commands

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release
```

The release executable will be located at `target/release/encfs.exe` (Windows) or `target/release/encfs` (Linux/macOS).

### Feature Flags

| Feature | Description |
|---------|-------------|
| `fuse` | Enables FUSE filesystem bindings (Linux/macOS only) |
| `fuse-integration` | Alias for `fuse` |

By default, no features are enabled, producing a standalone GUI application.

---

## Usage Instructions

### Creating a New Encrypted Image

1. Run `encfs.exe`.
2. Click **"Create New Image"** on the welcome screen.
3. Enter the full path for the image file (e.g., `C:\Users\admin\Documents\encfs.img`).
4. Set the image size in megabytes (e.g., `10` for 10 MB).
5. Enter a strong password and confirm it.
6. Click **"Create"**.
7. The browser screen will open, showing an empty filesystem.

### Opening an Existing Encrypted Image

1. Run `encfs.exe`.
2. Click **"Open Existing Image"** on the welcome screen.
3. Enter the full path to the existing image file.
4. Enter the password used when creating the image.
5. Click **"Open"**.
6. The browser screen will open, displaying the filesystem contents.

### Using the Browser

- **Navigation:** Click on folders to enter them. Click **"Up"** to go to the parent directory.
- **Viewing Files:** Click on a file to display its decrypted contents in the editor pane.
- **Editing Files:** Modify the content in the editor pane and click **"Save"** to encrypt and write changes back to the image.
- **Refresh:** Click **"Refresh"** to reload the current directory listing.

### Security Notes

- **Password Strength:** Use a strong, unique password. The master key is derived directly from the password via Argon2id.
- **Image Backup:** If the image file is corrupted or the password is lost, data is irrecoverable. Keep backups.
- **Memory Exposure:** While the application is running, decrypted data exists in memory. Ensure the system is secure.
- **No Plaintext on Disk:** All data written to the image file is encrypted. Temporary files or swap space may contain plaintext; consider full-disk encryption for the host system.

---

## Security Considerations

### Cryptographic Choices

- **ChaCha20-Poly1305:** An AEAD cipher providing 256-bit security. Chosen for its performance and resistance to timing attacks.
- **Argon2id:** The winner of the Password Hashing Competition. Memory-hard parameters (64 MiB, 3 passes, 4 threads) resist GPU/ASIC attacks.
- **HKDF-SHA256:** A standard key derivation function used to derive per-file keys from the master key.

### Nonce Management

Nonces are derived deterministically from `FileUUID + BlockIndex + NonceSequence` using HKDF-SHA256. This approach:
- Guarantees uniqueness per block.
- Avoids the need to store nonces on disk (saving space and reducing attack surface).
- Prevents nonce reuse, which would catastrophically break ChaCha20-Poly1305 security.

### Integrity Protection

Every 4 KiB block includes a 16-byte Poly1305 authentication tag. During decryption:
- If the tag does not match, the block is rejected.
- The error is propagated as `EIO` (Input/Output Error) to the caller.
- The application does not panic on authentication failure.

### Read-Modify-Write

When writing data that does not align to 4 KiB block boundaries:
1. The existing block is read from the image.
2. The block is decrypted into a plaintext buffer.
3. The new bytes are overlaid at the correct offset.
4. The modified plaintext buffer is re-encrypted with the **same nonce** (to maintain block identity).
5. The encrypted block is written back to the same image offset.

This ensures that partial writes do not corrupt adjacent data or break nonce uniqueness.

---

## Project Structure

```
encfs/
├── Cargo.toml              # Project manifest with dependencies and features
├── README.md               # This file
├── src/
│   ├── lib.rs              # Library root, module declarations
│   ├── main.rs             # GUI application (eframe/egui)
│   ├── error.rs            # Error types with POSIX errno mapping
│   ├── crypto/
│   │   ├── mod.rs          # Crypto module declarations
│   │   ├── kdf.rs          # Argon2id + HKDF key derivation
│   │   └── aead.rs         # ChaCha20-Poly1305 encrypt/decrypt
│   └── fs/
│       ├── mod.rs          # Filesystem module declarations
│       ├── image.rs        # Backing image file I/O
│       ├── allocator.rs    # Block allocation table (BAT)
│       ├── metadata.rs     # Inode table, directory entries, block map
│       └── passthrough.rs  # FUSE filesystem implementation (optional)
└── .kilo/
    └── plans/
        ├── system-design.md    # Detailed system design document
        └── implementation-plan.md # Implementation roadmap
```

---

## License

MIT
