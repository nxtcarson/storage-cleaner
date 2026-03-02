# Storage Cleaner

A native desktop app that scans drives for large files and unused applications, presenting results for selective cleanup. Built with Rust and egui.

## Requirements

- Rust (install from https://rustup.rs)
- Windows (for full Prefetch-based unused app detection; Big Files and Quick Clean work on any platform)

**Big Files (MFT scan):** On Windows NTFS drives, the app reads the Master File Table directly for much faster scanning (similar to Everything). This requires running as Administrator. If access is denied, it falls back to standard directory walking.

## Build

```bash
cd storage-cleaner
cargo build --release
```

## Run

```bash
cargo run --release
```

## Features

- **Big Files**: Scan a drive for files above a minimum size (default 50 MB), sort by size, select and delete to Recycle Bin
- **Unused Apps**: Find executables not run recently (uses Windows Prefetch when available), filter by days
- **Quick Clean**: One-click clear of temp folders (user temp, Windows temp, update cache)

All deletions go to Recycle Bin when possible.

## AI Features (OpenAI)

- **Ask AI**: Select a single file and click "Ask AI" to get advice on whether it's safe to delete
- **AI Suggest**: After a scan, click "AI Suggest" to get recommendations on which files are safest to delete first

**Setup:** Open the Settings tab and enter your OpenAI API key. It's stored locally at:
- Windows: `%APPDATA%\storage-cleaner\config.json`
- Linux/macOS: `~/.config/storage-cleaner/config.json`

Default model: `gpt-5-nano` (configurable in Settings).
