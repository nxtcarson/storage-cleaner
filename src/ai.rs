use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Option<Vec<ChatChoice>>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
}

pub fn ask_about_file(api_key: &str, model: &str, path: &str, size_bytes: u64) -> Result<String, String> {
    let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
    let prompt = format!(
        "A user is considering deleting this file to free up disk space. Based only on the path and size, \
        is this file likely important (system, app data, user documents) or safe to delete (cache, temp, downloads, etc)? \
        Reply in 2-3 sentences. Be concise.\n\nFile: {}\nSize: {:.1} MB",
        path, size_mb
    );

    let response = call_api(api_key, model, &prompt)?;
    Ok(response.trim().to_string())
}

pub fn suggest_deletions(
    api_key: &str,
    model: &str,
    entries: &[(String, u64)],
) -> Result<String, String> {
    let list: String = entries
        .iter()
        .take(50)
        .map(|(path, size)| format!("{} ({:.1} MB)", path, *size as f64 / (1024.0 * 1024.0)))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "A user scanned their drive for large files. Suggest which of these are SAFEST to delete first \
        (caches, temp, old downloads) vs which to KEEP (system, app data, documents). \
        List 5-10 files that are safe to delete, with brief reason. Be concise.\n\nFiles:\n{}",
        list
    );

    let response = call_api(api_key, model, &prompt)?;
    Ok(response.trim().to_string())
}

fn call_api(api_key: &str, model: &str, content: &str) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("OpenAI API key not set. Add it in Settings.".to_string());
    }

    let client = reqwest::blocking::Client::new();
    let req = ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: "You are a helpful assistant that advises on file safety for disk cleanup.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: content.to_string(),
            },
        ],
    };

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&req)
        .send()
        .map_err(|e| {
            let msg = e.to_string();
            if let Some(source) = e.source() {
                format!("{}: {}", msg, source)
            } else {
                msg
            }
        })?;

    let status = response.status();
    let body: ChatResponse = response.json().map_err(|e| e.to_string())?;

    if let Some(err) = body.error {
        return Err(err.message);
    }

    let text = body
        .choices
        .and_then(|c| c.into_iter().next())
        .map(|c| c.message.content)
        .ok_or_else(|| format!("Unexpected API response: {}", status))?;

    Ok(text)
}
