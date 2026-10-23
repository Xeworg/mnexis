//! The workspace key: the identity of a directory, in one canonical form.
//!
//! The key is what a binding is recorded under, so the form has to be
//! byte-identical everywhere: the human who writes it, the engine that
//! stores it, the agent extension that looks it up, and the resolver that
//! compares it. This module is the one place that decides what a key is.
//!
//! It is pure text on purpose: the rule belongs to the domain, so the
//! registry (which stores keys) and the resolver (which reads them) can
//! share it without depending on each other.

/// Normalize a git remote URL into the canonical key suffix.
///
/// Drop a `git+` prefix, drop the protocol, drop the `user@` part of an
/// scp-style remote, drop a trailing `.git`, lowercase everything, replace
/// `/` and `:` runs with `-`, collapse repeated dashes, and trim the dashes
/// at both ends. The host's dots survive:
///
/// ```text
/// https://github.com/Xeworg/AegisPI.git  ->  github.com-xeworg-aegispi
/// git@github.com:Xeworg/AegisPI.git      ->  github.com-xeworg-aegispi
/// ```
///
/// `None` means the remote has no usable path left (empty or only
/// separators); the caller falls back to the next key rule.
pub fn normalize_remote_url(remote: &str) -> Option<String> {
    let mut url = remote.trim();
    if url.is_empty() {
        return None;
    }
    if let Some(rest) = strip_prefix_ci(url, "git+") {
        url = rest;
    }
    if let Some(rest) = after_protocol(url) {
        url = rest;
    }
    if let Some(rest) = after_user_prefix(url) {
        url = rest;
    }
    if let Some(rest) = strip_suffix_ci(url, ".git") {
        url = rest;
    }

    let mut out = String::with_capacity(url.len());
    for c in url.to_lowercase().chars() {
        if matches!(c, '/' | ':' | '-') {
            // A run of separators becomes one dash, and no dash opens the key.
            if out.is_empty() || out.ends_with('-') {
                continue;
            }
            out.push('-');
        } else {
            out.push(c);
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { None } else { Some(out) }
}

/// `s` without a leading `prefix`, ASCII-case-insensitively.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let bytes = s.as_bytes();
    let p = prefix.as_bytes();
    if bytes.len() >= p.len() && bytes[..p.len()].eq_ignore_ascii_case(p) {
        Some(&s[p.len()..])
    } else {
        None
    }
}

/// `s` without a trailing `suffix`, ASCII-case-insensitively.
fn strip_suffix_ci<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let bytes = s.as_bytes();
    let suf = suffix.as_bytes();
    if bytes.len() >= suf.len() && bytes[bytes.len() - suf.len()..].eq_ignore_ascii_case(suf) {
        Some(&s[..bytes.len() - suf.len()])
    } else {
        None
    }
}

/// `url` without a `scheme://` prefix, when the prefix is a real scheme.
fn after_protocol(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    Some(rest)
}

/// The canonical form of a workspace key.
///
/// One rule, in one place: a key is `git:<normalized remote>` or
/// `path:<absolute directory>`, and the remote is normalized the same way
/// the pi extension normalizes it. Every form of the same workspace lands
/// on the same key, so a binding recorded by a human with the raw remote
/// (`git:github.com/owner/repo`) is the binding the agent looks up
/// (`git:github.com-owner-repo`).
///
/// The function is idempotent: canonicalizing a canonical key changes
/// nothing.
pub fn canonical_workspace_key(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return String::new();
    }
    if let Some(rest) = key.strip_prefix("git:") {
        return match normalize_remote_url(rest) {
            Some(normalized) => format!("git:{normalized}"),
            None => key.to_string(),
        };
    }
    if key.starts_with("path:") {
        return key.to_string();
    }
    // A bare remote: the human wrote the URL, not the key.
    match normalize_remote_url(key) {
        Some(normalized) => format!("git:{normalized}"),
        None => key.to_string(),
    }
}

/// `url` without a leading `user@` part (scp-style remotes).
fn after_user_prefix(url: &str) -> Option<&str> {
    let at = url.find('@')?;
    if at == 0 || url[..at].contains('/') {
        return None;
    }
    Some(&url[at + 1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- normalization: the vectors the extension's tests pin ---

    #[test]
    fn normalization_strips_the_protocol_the_git_suffix_and_lowercases() {
        assert_eq!(
            normalize_remote_url("https://github.com/Xeworg/AegisPI.git").as_deref(),
            Some("github.com-xeworg-aegispi")
        );
    }

    #[test]
    fn normalization_handles_scp_style_ssh_remotes() {
        assert_eq!(
            normalize_remote_url("git@github.com:Xeworg/AegisPI.git").as_deref(),
            Some("github.com-xeworg-aegispi")
        );
    }

    #[test]
    fn normalization_handles_the_ssh_protocol() {
        assert_eq!(
            normalize_remote_url("ssh://git@github.com/Xeworg/AegisPI.git").as_deref(),
            Some("github.com-xeworg-aegispi")
        );
    }

    #[test]
    fn normalization_keeps_the_host_dots_and_replaces_slashes_and_colons() {
        assert_eq!(
            normalize_remote_url("https://gitlab.com/group/sub/proj").as_deref(),
            Some("gitlab.com-group-sub-proj")
        );
    }

    #[test]
    fn normalization_collapses_repeated_separators() {
        assert_eq!(
            normalize_remote_url("https://github.com//Xeworg//AegisPI").as_deref(),
            Some("github.com-xeworg-aegispi")
        );
    }

    #[test]
    fn normalization_rejects_a_remote_without_a_usable_path() {
        // The extension throws on exactly these inputs, so the CLI has to
        // answer `None` for them: a key the two sides disagree on is worse
        // than no key at all.
        assert_eq!(normalize_remote_url(""), None);
        assert_eq!(normalize_remote_url("   "), None);
        assert_eq!(normalize_remote_url("https://"), None);
        assert_eq!(normalize_remote_url("/"), None);
        assert_eq!(normalize_remote_url("::::"), None);
    }

    // --- the key: the same three cases, in the same order, as the extension ---
}
