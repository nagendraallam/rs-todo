use std::{fs, io, path::PathBuf};

use crate::todo::TodoList;

// XOR key — keeps the file non-human-readable without heavy crypto deps.
// 32 bytes exactly.
const KEY: &[u8] = b"T0d0Cli_XorEncryptionKey_32Bytes";

fn data_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".todo_store")
}

pub fn load() -> TodoList {
    let path = data_path();
    if !path.exists() {
        return TodoList::default();
    }
    let enc = fs::read(&path).unwrap_or_default();
    let raw = xor(&enc);
    serde_json::from_slice(&raw).unwrap_or_default()
}

pub fn save(list: &TodoList) -> io::Result<()> {
    let raw = serde_json::to_vec(list)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    fs::write(data_path(), xor(&raw))
}

fn xor(data: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, &b)| b ^ KEY[i % KEY.len()])
        .collect()
}
