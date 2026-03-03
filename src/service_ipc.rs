#[cfg(windows)]
pub mod windows {
    use std::io::{BufRead, BufReader, Write};
    use std::path::PathBuf;

    use chrono::TimeZone;
    use interprocess::local_socket::prelude::*;
    use interprocess::local_socket::{ConnectOptions, GenericNamespaced, ListenerOptions, ToNsName};
    use serde::{Deserialize, Serialize};

    use crate::scanner::FileEntry;

    pub const PIPE_NAME: &str = "storage-cleaner-scan";

    #[derive(Debug, Serialize, Deserialize)]
    pub enum Request {
        Scan { drive: char },
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct FileEntryWire {
        pub path: PathBuf,
        pub size_bytes: u64,
        pub extension: String,
        pub last_modified: Option<i64>,
        pub category: String,
    }

    impl From<&FileEntry> for FileEntryWire {
        fn from(e: &FileEntry) -> Self {
            FileEntryWire {
                path: e.path.clone(),
                size_bytes: e.size_bytes,
                extension: e.extension.clone(),
                last_modified: e.last_modified.map(|t| t.timestamp()),
                category: format!("{:?}", e.category),
            }
        }
    }

    impl FileEntryWire {
        pub fn to_entry(&self) -> FileEntry {
            use crate::scanner::FileCategory;
            let category = match self.category.as_str() {
                "Documents" => FileCategory::Documents,
                "Media" => FileCategory::Media,
                "Archives" => FileCategory::Archives,
                "Executables" => FileCategory::Executables,
                "System" => FileCategory::System,
                "Temp" => FileCategory::Temp,
                "DevBuild" => FileCategory::DevBuild,
                _ => FileCategory::Other,
            };
            FileEntry {
                path: self.path.clone(),
                size_bytes: self.size_bytes,
                extension: self.extension.clone(),
                last_modified: self
                    .last_modified
                    .and_then(|s| chrono::Utc.timestamp_opt(s, 0).single()),
                category,
            }
        }
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub enum Response {
        Ok,
        Error(String),
        File(FileEntryWire),
        Done,
    }

    pub fn create_listener() -> Result<LocalSocketListener, std::io::Error> {
        let name = PIPE_NAME.to_ns_name::<GenericNamespaced>().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string())
        })?;
        ListenerOptions::new()
            .name(name)
            .create_sync()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e))
    }

    pub fn connect_client() -> Result<LocalSocketStream, std::io::Error> {
        let name = PIPE_NAME.to_ns_name::<GenericNamespaced>().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string())
        })?;
        ConnectOptions::new()
            .name(name)
            .connect_sync()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e))
    }

    pub fn recv_request(stream: &mut LocalSocketStream) -> std::io::Result<Request> {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Empty request",
            ));
        }
        serde_json::from_str(line).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }

    pub fn send_request(stream: &mut LocalSocketStream, req: &Request) -> std::io::Result<()> {
        let line = serde_json::to_string(req).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        writeln!(stream, "{}", line)?;
        stream.flush()?;
        Ok(())
    }

    pub fn recv_response(stream: &mut LocalSocketStream) -> std::io::Result<Response> {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Empty response",
            ));
        }
        serde_json::from_str(line).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }

    pub fn send_response(stream: &mut impl Write, resp: &Response) -> std::io::Result<()> {
        let line = serde_json::to_string(resp).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        writeln!(stream, "{}", line)?;
        stream.flush()?;
        Ok(())
    }
}
