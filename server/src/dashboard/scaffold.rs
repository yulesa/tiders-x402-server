//! Scaffold a dashboard project directory from embedded templates.
//!
//! ## File categories
//!
//! **Managed files** — all entries in `TEMPLATES` are written on every scaffold run.
//!
//! **User-owned files** — currently only `pages/index.md`. Written once on
//! first scaffold; never touched again regardless of `--force`.
//!
//! ## `--force` behaviour
//!
//! Without `--force` the scaffolder bails if the project directory is
//! non-empty, so it never clobbers an existing project accidentally.
//!
//! With `--force` the scaffolder reads `.tiders-managed.json` from the
//! previous run and compares the sha256 of each managed file on disk against
//! the recorded hash:
//!
//! - **Matches recorded hash** (unmodified) — overwrite silently.
//! - **Differs from recorded hash** (user-edited) — copy to `.old/<filename>`
//!   in the project root (overwriting any previous `.old` copy), then
//!   overwrite with the new template. A warning is logged for each such file.
//! - **Missing** — written fresh, no backup needed.
//!
//! If `.tiders-managed.json` is absent or unparseable the scaffolder treats
//! every file as unmodified and overwrites all of them without backing up.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use super::config::{ScaffoldInput, ScaffoldResult};
use super::templates;

/// Scaffold one dashboard project.
pub fn scaffold_dashboard_folder(input: &ScaffoldInput<'_>) -> Result<ScaffoldResult> {
    let project_dir = input.project_dir.to_path_buf();

    // Without --force, refuse to touch a non-empty existing directory.
    if project_dir.exists() {
        let is_empty = std::fs::read_dir(&project_dir)
            .with_context(|| format!("Failed to read directory {}", project_dir.display()))?
            .next()
            .is_none();
        if !is_empty && !input.force {
            bail!(
                "Directory {} already exists and is not empty.\n\
                 Use --force to overwrite managed files (user-owned files like \
                 pages/*.md and sources/**/*.sql are preserved).",
                project_dir.display()
            );
        }
    }

    std::fs::create_dir_all(&project_dir)
        .with_context(|| format!("Failed to create {}", project_dir.display()))?;

    // Load recorded hashes from the previous run so --force can detect drift.
    let recorded = if input.force {
        read_manifest(&project_dir)
    } else {
        HashMap::new()
    };

    let mut written: Vec<String> = Vec::new();
    let mut preserved: Vec<String> = Vec::new();
    let mut backed_up: Vec<String> = Vec::new();
    let mut managed_entries: Vec<(String, String)> = Vec::new();

    // Build the full list of (relative-path, contents) for every managed file.
    let mut managed: Vec<(String, String)> = Vec::new();

    for tpl in templates::TEMPLATES {
        let contents = if tpl.substitute {
            templates::render(tpl.contents, input.name, input.seed_table, input.source_name)
        } else {
            tpl.contents.to_string()
        };
        managed.push((tpl.path.to_string(), contents));
    }
    for (rel, contents) in &input.rendered_files {
        managed.push((rel.to_string_lossy().into_owned(), contents.clone()));
    }

    // Write each managed file, backing up user-modified ones when --force.
    for (rel, contents) in &managed {
        let path = project_dir.join(rel);
        let new_hash = sha256_hex(contents.as_bytes());

        if input.force && path.exists() {
            let current_hash = hash_file(&path)?;
            let clean = recorded.get(rel.as_str()).map_or(false, |h| *h == current_hash);
            if !clean {
                // User has edited this file — back it up before overwriting.
                backup_file(&project_dir, &path, rel)?;
                backed_up.push(rel.clone());
                tracing::warn!(
                    "modified: {} was modified since last scaffold, stored in .old/{}",
                    rel,
                    filename_of(rel)
                );
            }
        }

        write_file(&path, contents)?;
        managed_entries.push((rel.clone(), new_hash));
        written.push(rel.clone());
    }

    // User-owned: pages/index.md is written only if missing, never on --force.
    let index_md_rel = "pages/index.md";
    let index_md_path = project_dir.join(index_md_rel);
    if index_md_path.exists() {
        preserved.push(index_md_rel.to_string());
    } else {
        let starter = templates::render(templates::STARTER_INDEX_MD, input.name, input.seed_table, input.source_name);
        write_file(&index_md_path, &starter)?;
        written.push(index_md_rel.to_string());
    }

    // Always rewrite the manifest with the hashes from this run.
    let manifest = manifest_json(input.name, &managed_entries);
    write_file(&project_dir.join(".tiders-managed.json"), &manifest)?;
    written.push(".tiders-managed.json".to_string());

    Ok(ScaffoldResult {
        project_dir,
        written,
        preserved,
        backed_up,
    })
}

/// Reads `.tiders-managed.json` and returns a map of relative path → sha256.
/// Returns an empty map if the file is missing or cannot be parsed.
fn read_manifest(project_dir: &Path) -> HashMap<String, String> {
    let path = project_dir.join(".tiders-managed.json");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    parse_manifest_hashes(&raw)
}

/// Minimal manifest parser — extracts `{"path": ..., "sha256": ...}` entries
/// without pulling in a JSON dependency. Returns an empty map on any error.
fn parse_manifest_hashes(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    // Each entry looks like: { "path": "foo/bar", "sha256": "abcd..." }
    for line in raw.lines() {
        let line = line.trim();
        if let (Some(path), Some(sha)) = (extract_json_str(line, "path"), extract_json_str(line, "sha256")) {
            map.insert(path, sha);
        }
    }
    map
}

/// Extracts the string value of a JSON key from a single line of the form
/// `{ "key": "value", ... }`. Sufficient for the manifest format we write.
fn extract_json_str(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let start = line.find(&needle)? + needle.len();
    let rest = line[start..].trim();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Copies `file_path` to `<project_dir>/.old/<filename>`.
fn backup_file(project_dir: &Path, file_path: &Path, rel: &str) -> Result<()> {
    let old_dir = project_dir.join(".old");
    std::fs::create_dir_all(&old_dir)
        .with_context(|| format!("Failed to create {}", old_dir.display()))?;
    let dest = old_dir.join(filename_of(rel));
    std::fs::copy(file_path, &dest)
        .with_context(|| format!("Failed to back up {} to {}", file_path.display(), dest.display()))?;
    Ok(())
}

/// Returns just the filename component of a relative path string.
fn filename_of(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn hash_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    std::fs::write(path, contents)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn manifest_json(dashboard_name: &str, managed: &[(String, String)]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "{{");
    let _ = writeln!(out, "  \"schema_version\": 1,");
    let _ = writeln!(out, "  \"dashboard_name\": \"{}\",", json_escape(dashboard_name));
    let _ = writeln!(out, "  \"managed_files\": [");
    for (i, (path, sha)) in managed.iter().enumerate() {
        let comma = if i + 1 == managed.len() { "" } else { "," };
        let _ = writeln!(
            out,
            "    {{ \"path\": \"{}\", \"sha256\": \"{sha}\" }}{comma}",
            json_escape(path)
        );
    }
    let _ = writeln!(out, "  ]");
    let _ = writeln!(out, "}}");
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
