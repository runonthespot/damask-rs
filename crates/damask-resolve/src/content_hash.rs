use sha2::{Digest, Sha256};

/// Compute the truncated SHA-256 content hash (first 12 hex chars).
/// This is the content_hash stored on spans per spec §4.1.
pub fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    // First 6 bytes = 12 hex chars
    hex_encode(&result[..6])
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_length() {
        let hash = content_hash("fn main() {}");
        assert_eq!(hash.len(), 12);
    }

    #[test]
    fn hash_is_deterministic() {
        let h1 = content_hash("fn main() {}");
        let h2 = content_hash("fn main() {}");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_differs_for_different_content() {
        let h1 = content_hash("fn main() {}");
        let h2 = content_hash("fn main() { println!(\"hello\"); }");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_is_hex() {
        let hash = content_hash("test content");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
