/// CLI-level error type wrapping all crate errors.
/// Uses anyhow for user-facing messages with context.
pub type Result<T> = anyhow::Result<T>;
