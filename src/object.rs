use std::path::{Path, PathBuf};

// ── Core model ───────────────────────────────────────────────────────────────
//
//  Object  ─── can point to other Objects (via [[name]] links in content)
//    ├── Node   – a file  (.md or any text file), holds note content
//    └── Group  – a directory, contains child Objects
//
// Both implement the `Object` trait so they can be handled uniformly.

/// Every node and group is an Object.
pub trait Object {
    /// Display name (file/dir name without path).
    fn name(&self) -> &str;

    /// Absolute path on disk.
    fn path(&self) -> &Path;

    /// Whether this object is a group (directory).
    fn is_group(&self) -> bool;

    /// Text content of this object.
    /// - Node  → file contents
    /// - Group → auto-generated index listing its children
    fn content(&self) -> String;

    /// Outgoing links: `[[target]]` references found in content.
    fn links(&self) -> Vec<String> {
        extract_links(&self.content())
    }
}

// ── Node (file = note) ───────────────────────────────────────────────────────

pub struct Node {
    pub path: PathBuf,
    pub name: String,
}

impl Node {
    pub fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Self { path, name }
    }
}

impl Object for Node {
    fn name(&self) -> &str { &self.name }
    fn path(&self) -> &Path { &self.path }
    fn is_group(&self) -> bool { false }
    fn content(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }
}

// ── Group (directory = container) ────────────────────────────────────────────

pub struct Group {
    pub path: PathBuf,
    pub name: String,
}

impl Group {
    pub fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        Self { path, name }
    }

    /// Returns direct children as `ObjectKind` variants.
    pub fn children(&self) -> Vec<ObjectKind> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.path) else { return out };
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let p = entry.path();
            if p.is_dir() {
                out.push(ObjectKind::Group(Group::new(p)));
            } else {
                out.push(ObjectKind::Node(Node::new(p)));
            }
        }
        out
    }
}

impl Object for Group {
    fn name(&self) -> &str { &self.name }
    fn path(&self) -> &Path { &self.path }
    fn is_group(&self) -> bool { true }

    /// Auto-index: lists children and any [[links]] stored in a `.group` meta file.
    fn content(&self) -> String {
        // Optional per-group note stored as `.group` inside the directory
        let meta = self.path.join(".group");
        let note = if meta.exists() {
            std::fs::read_to_string(&meta).unwrap_or_default()
        } else {
            String::new()
        };

        let children = self.children();
        let mut index = format!("# Group: {}\n\n", self.name);
        if !note.is_empty() {
            index.push_str(&note);
            index.push_str("\n\n");
        }
        index.push_str("## Contents\n\n");
        for child in &children {
            let (icon, name) = match child {
                ObjectKind::Node(n)  => ("📄", n.name()),
                ObjectKind::Group(g) => ("📁", g.name()),
            };
            index.push_str(&format!("- {icon} [[{name}]]\n"));
        }
        index
    }
}

// ── ObjectKind enum — uniform handle for either variant ──────────────────────

pub enum ObjectKind {
    Node(Node),
    Group(Group),
}

impl ObjectKind {
    pub fn as_object(&self) -> &dyn Object {
        match self {
            ObjectKind::Node(n)  => n,
            ObjectKind::Group(g) => g,
        }
    }

    pub fn path(&self) -> &Path {
        self.as_object().path()
    }

    pub fn name(&self) -> &str {
        self.as_object().name()
    }

    pub fn is_group(&self) -> bool {
        self.as_object().is_group()
    }

    pub fn content(&self) -> String {
        self.as_object().content()
    }

    pub fn links(&self) -> Vec<String> {
        self.as_object().links()
    }

    /// Save text content back to disk.
    /// For a Node  → overwrites the file.
    /// For a Group → writes to the `.group` meta file inside the directory.
    pub fn save_content(&self, text: &str) -> std::io::Result<()> {
        match self {
            ObjectKind::Node(n)  => std::fs::write(&n.path, text),
            ObjectKind::Group(g) => std::fs::write(g.path.join(".group"), text),
        }
    }
}

// ── Link extraction ──────────────────────────────────────────────────────────

/// Finds all `[[target]]` references in `text`.
pub fn extract_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("]]") {
            links.push(rest[..end].trim().to_string());
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    links
}