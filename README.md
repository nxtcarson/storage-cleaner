# Storage Cleaner

A native desktop app that scans drives for large files and unused applications, presenting results for selective cleanup. Built with Rust and egui.

## Requirements

- Rust (install from https://rustup.rs)
- Windows: Visual Studio Build Tools with C++ workload (for MFT scanning)
- Windows (for full Prefetch-based unused app detection; Disk Analysis and Quick Clean work on any platform)

**Disk Analysis (MFT scan):** On Windows NTFS drives, the app uses usn-journal-rs for fast MFT enumeration (similar to Everything). This requires running as Administrator. If access is denied, it falls back to parallel directory walking.

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

- **Disk Analysis**: Fast drive scan with insights by extension, folder, and category. Largest files and stale files (6+ months old) views. AI automatically analyzes top files and suggests what's safe to delete. Save snapshots for later comparison.
- **Unused Apps**: Find executables not run recently (uses Windows Prefetch when available), filter by days
- **Quick Clean**: One-click clear of temp folders (user temp, Windows temp, update cache)

All deletions go to Recycle Bin when possible.

## AI Features (OpenAI)

- **Auto AI**: After a scan, the top ~50 largest files are automatically sent to the AI for analysis
- **Structured verdicts**: Each file gets a verdict (safe_to_delete, review, keep) with reasoning
- **Review Suggestions**: Files grouped by verdict; select and delete in one action

**Setup:** Open the Settings tab and enter your OpenAI API key. It's stored locally at:
- Windows: `%APPDATA%\storage-cleaner\config.json`
- Linux/macOS: `~/.config/storage-cleaner/config.json`

Default model: `gpt-5-nano` (configurable in Settings).
