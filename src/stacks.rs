//! Stacks: notes collected from what you read. Select text in any app, hold
//! the Stacks shortcut, and say what you think about it; the quote and your
//! words join the active stack. Notes are kept as one small owner-only JSON
//! file (`stacks.json` in the data folder) so a stack can be pasted as
//! Markdown, or opened as one.
//!
//! The store is deliberately plain: one file, four named stacks, and notes
//! holding a quote and a body. Every read and write goes through the file
//! under one process-wide lock, so the listener that adds a spoken note and
//! the window that shows the stack never hold different copies. Writes are
//! atomic and owner-only, as History's are; a malformed file is preserved
//! beside the new one rather than silently overwritten.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// How many stacks there are, as Sendpoint's four.
pub const STACK_COUNT: usize = 4;
const VERSION: u32 = 1;

/// The most notes one stack holds; the oldest goes first.
pub const MAX_NOTES: usize = 500;
/// Bounds on one note's quote and body. They keep the file small and a
/// runaway selection or dictation from filling the disk.
pub const MAX_QUOTE_BYTES: usize = 8 * 1024;
pub const MAX_BODY_BYTES: usize = 8 * 1024;

/// The default stack names, in order.
const DEFAULT_NAMES: [&str; STACK_COUNT] = ["Reading notes", "Ideas", "Questions", "Scratch"];

/// One note: the text it was written about, and what was said or typed.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Note {
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub body: String,
}

impl Note {
    pub fn new(quote: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            quote: quote.into(),
            body: body.into(),
        }
    }

    /// A note with neither a quote nor a body carries nothing; it is never
    /// stored.
    pub fn is_empty(&self) -> bool {
        self.quote.trim().is_empty() && self.body.trim().is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Stack {
    #[serde(default)]
    name: String,
    #[serde(default)]
    notes: Vec<Note>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Saved {
    version: u32,
    #[serde(default)]
    active: usize,
    #[serde(default)]
    stacks: Vec<Stack>,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            version: VERSION,
            active: 0,
            stacks: DEFAULT_NAMES
                .into_iter()
                .map(|name| Stack {
                    name: name.into(),
                    notes: Vec::new(),
                })
                .collect(),
        }
    }
}

/// One stack as the window reads it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackView {
    pub name: String,
    pub notes: Vec<Note>,
}

/// The whole store at one moment: every stack and which one is active.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub active: usize,
    pub stacks: Vec<StackView>,
}

impl Default for Snapshot {
    fn default() -> Self {
        snapshot_of(&Saved::default())
    }
}

impl Snapshot {
    pub fn active_stack(&self) -> Option<&StackView> {
        self.stacks.get(self.active)
    }

    /// One stack's notes as Markdown, as the paste and the file show them.
    pub fn markdown(&self, index: usize) -> String {
        self.stacks
            .get(index)
            .map(|stack| compose(&stack.notes))
            .unwrap_or_default()
    }
}

/// The shared handle over `stacks.json`. Every operation rereads the file,
/// so a listener and a window sharing one handle, or two handles, always
/// see each other's changes.
#[derive(Clone)]
pub struct Stacks {
    path: Arc<PathBuf>,
}

/// Serializes this process's reads and read-modify-writes. Hex has one
/// process per data folder, so no cross-process lock is needed.
static STORE: Mutex<()> = Mutex::new(());

impl Stacks {
    pub fn open(path: PathBuf) -> Self {
        Self {
            path: Arc::new(path),
        }
    }

    /// `stacks.json` in Hex's data folder.
    pub fn open_default() -> io::Result<Self> {
        let directory =
            crate::app_paths::support_dir().map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Self::open(directory.join("stacks.json")))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every stack and the active one, read from disk. A missing file is
    /// the default four empty stacks; a malformed one is preserved on the
    /// next write (see [`Self::persist`]).
    pub fn snapshot(&self) -> Snapshot {
        let _guard = STORE.lock().unwrap_or_else(|error| error.into_inner());
        snapshot_of(&normalize(self.read()))
    }

    /// Appends a note to the active stack. Notes with neither words nor a
    /// quote are not stored.
    pub fn add(&self, note: Note) -> io::Result<Snapshot> {
        self.change(|saved| {
            if note.is_empty() {
                return;
            }
            if let Some(stack) = saved.stacks.get_mut(saved.active) {
                stack.notes.push(note);
                let excess = stack.notes.len().saturating_sub(MAX_NOTES);
                stack.notes.drain(..excess);
            }
        })
    }

    /// Makes stack `index` the one notes are added to and the one pasted.
    pub fn use_stack(&self, index: usize) -> io::Result<Snapshot> {
        self.change(|saved| {
            if index < STACK_COUNT {
                saved.active = index;
            }
        })
    }

    /// Empties the active stack.
    pub fn clear(&self) -> io::Result<Snapshot> {
        self.change(|saved| {
            if let Some(stack) = saved.stacks.get_mut(saved.active) {
                stack.notes.clear();
            }
        })
    }

    /// Empties stack `index`, wherever the active one has moved to.
    pub fn clear_at(&self, index: usize) -> io::Result<Snapshot> {
        self.change(|saved| {
            if let Some(stack) = saved.stacks.get_mut(index) {
                stack.notes.clear();
            }
        })
    }

    /// Renames the active stack. An empty name keeps the old one.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn rename(&self, name: &str) -> io::Result<Snapshot> {
        let name = name.trim();
        self.change(|saved| {
            if !name.is_empty()
                && let Some(stack) = saved.stacks.get_mut(saved.active)
            {
                stack.name = name.to_string();
            }
        })
    }

    /// Applies `change` to a fresh read, then writes the whole store.
    fn change(&self, change: impl FnOnce(&mut Saved)) -> io::Result<Snapshot> {
        let _guard = STORE.lock().unwrap_or_else(|error| error.into_inner());
        let mut saved = self.read();
        change(&mut saved);
        let saved = normalize(saved);
        self.persist(&saved)?;
        Ok(snapshot_of(&saved))
    }

    fn read(&self) -> Saved {
        match fs::read(&*self.path) {
            Ok(bytes) => match serde_json::from_slice::<Saved>(&bytes) {
                Ok(saved) if saved.version == VERSION => saved,
                Ok(saved) => {
                    tracing::debug!(
                        version = saved.version,
                        "unsupported stacks version; keeping the file until the next change"
                    );
                    Saved::default()
                }
                Err(error) => {
                    tracing::debug!(
                        %error,
                        path = %self.path.display(),
                        "stacks file is malformed; the next change preserves it"
                    );
                    Saved::default()
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Saved::default(),
            Err(error) => {
                tracing::debug!(%error, path = %self.path.display(), "could not read stacks");
                Saved::default()
            }
        }
    }

    fn persist(&self, saved: &Saved) -> io::Result<()> {
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        create_owner_directory(parent)?;
        // A file Hex could not parse stays beside the new one, once.
        let malformed = match fs::read(&*self.path) {
            Ok(bytes) => !matches!(
                serde_json::from_slice::<Saved>(&bytes),
                Ok(saved) if saved.version == VERSION
            ),
            Err(_) => false,
        };
        if malformed {
            let backup = self.path.with_extension("json.corrupt");
            match fs::rename(&*self.path, &backup) {
                Ok(()) => tracing::warn!(path = %backup.display(), "kept a malformed stacks file"),
                Err(error) => {
                    tracing::warn!(%error, "could not preserve the malformed stacks file")
                }
            }
        }
        let json = serde_json::to_vec(saved).map_err(io::Error::other)?;
        let temporary = self.path.with_extension("json.tmp");
        {
            let mut file = open_owner_only(&temporary)?;
            file.write_all(&json)?;
            file.sync_all()?;
        }
        if let Err(error) = fs::rename(&temporary, &*self.path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        crate::durable::sync_directory(parent)
    }
}

fn snapshot_of(saved: &Saved) -> Snapshot {
    Snapshot {
        active: saved.active,
        stacks: saved
            .stacks
            .iter()
            .map(|stack| StackView {
                name: stack.name.clone(),
                notes: stack.notes.clone(),
            })
            .collect(),
    }
}

/// Exactly four named stacks, one active, every note bounded.
fn normalize(mut saved: Saved) -> Saved {
    saved.version = VERSION;
    saved.stacks.truncate(STACK_COUNT);
    while saved.stacks.len() < STACK_COUNT {
        let index = saved.stacks.len();
        saved.stacks.push(Stack {
            name: DEFAULT_NAMES[index].into(),
            notes: Vec::new(),
        });
    }
    for (index, stack) in saved.stacks.iter_mut().enumerate() {
        if stack.name.trim().is_empty() {
            stack.name = DEFAULT_NAMES[index].into();
        }
        for note in &mut stack.notes {
            note.quote = clipped(note.quote.trim(), MAX_QUOTE_BYTES);
            note.body = clipped(note.body.trim(), MAX_BODY_BYTES);
        }
        stack.notes.retain(|note| !note.is_empty());
        let excess = stack.notes.len().saturating_sub(MAX_NOTES);
        stack.notes.drain(..excess);
    }
    saved.active = saved.active.min(STACK_COUNT - 1);
    saved
}

/// `text`, cut to at most `max` bytes on a character boundary.
fn clipped(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    text[..text.floor_char_boundary(max)].to_string()
}

/// A stack's notes as Markdown: each quote as a blockquote, its body after,
/// and a rule between notes.
pub fn compose(notes: &[Note]) -> String {
    notes
        .iter()
        .filter(|note| !note.is_empty())
        .map(|note| {
            [blockquote(&note.quote), note.body.trim().to_string()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

fn blockquote(quote: &str) -> String {
    let quote = quote.trim();
    if quote.is_empty() {
        return String::new();
    }
    quote
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What Open stack writes: the stack's name, then exactly what Paste sends.
#[cfg_attr(not(test), allow(dead_code))]
pub fn document(name: &str, notes: &[Note]) -> String {
    let body = compose(notes);
    format!(
        "# {name}\n\n{}\n",
        if body.is_empty() {
            "_No notes yet._"
        } else {
            &body
        }
    )
}

#[cfg(unix)]
fn open_owner_only(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_owner_only(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

#[cfg(unix)]
fn create_owner_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_owner_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp(name: &str) -> Stacks {
        let directory =
            std::env::temp_dir().join(format!("hex-stacks-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        Stacks::open(directory.join("stacks.json"))
    }

    #[test]
    fn a_new_store_has_four_named_stacks() {
        let snapshot = temp("new").snapshot();
        assert_eq!(snapshot.active, 0);
        assert_eq!(
            snapshot
                .stacks
                .iter()
                .map(|stack| stack.name.as_str())
                .collect::<Vec<_>>(),
            DEFAULT_NAMES
        );
        assert!(snapshot.stacks.iter().all(|stack| stack.notes.is_empty()));
    }

    #[test]
    fn notes_join_the_active_stack_and_survive_a_reopen() {
        let store = temp("round-trip");
        store
            .add(Note::new("  The quote  ", "  The note  "))
            .unwrap();
        store.add(Note::new("", "A note with no quote")).unwrap();
        store.use_stack(2).unwrap();
        store.add(Note::new("Later", "Elsewhere")).unwrap();

        let reopened = Stacks::open(store.path().to_path_buf());
        let snapshot = reopened.snapshot();
        assert_eq!(snapshot.active, 2);
        assert_eq!(
            snapshot.active_stack().unwrap().notes,
            [Note::new("Later", "Elsewhere")]
        );
        assert_eq!(
            snapshot.stacks[0].notes,
            [
                Note::new("The quote", "The note"),
                Note::new("", "A note with no quote"),
            ]
        );
    }

    #[test]
    fn empty_notes_are_not_stored() {
        let store = temp("empty");
        store.add(Note::new("", "   ")).unwrap();
        assert!(store.snapshot().active_stack().unwrap().notes.is_empty());
    }

    #[test]
    fn a_stack_pastes_as_markdown() {
        let notes = [
            Note::new("First quote", "First body"),
            Note::new("", "Only a body"),
            Note::new("Only\na quote", ""),
        ];
        assert_eq!(
            compose(&notes),
            "> First quote\n\nFirst body\n\n---\n\nOnly a body\n\n---\n\n> Only\n> a quote"
        );
        assert_eq!(
            document("Ideas", &notes),
            "# Ideas\n\n> First quote\n\nFirst body\n\n---\n\nOnly a body\n\n---\n\n> Only\n> a quote\n"
        );
        assert_eq!(document("Ideas", &[]), "# Ideas\n\n_No notes yet._\n");
    }

    #[test]
    fn clearing_and_renaming_act_on_the_active_stack_only() {
        let store = temp("clear");
        store.add(Note::new("One", "Two")).unwrap();
        store.use_stack(1).unwrap();
        store.add(Note::new("Three", "Four")).unwrap();
        store.rename("Pitches").unwrap();
        let snapshot = store.clear().unwrap();
        assert_eq!(snapshot.stacks[1].name, "Pitches");
        assert!(snapshot.stacks[1].notes.is_empty());
        assert_eq!(snapshot.stacks[0].notes.len(), 1);
        // An empty name keeps the old one.
        assert_eq!(store.rename("  ").unwrap().stacks[1].name, "Pitches");
    }

    #[test]
    fn notes_are_bounded_and_the_oldest_go_first() {
        let store = temp("bounded");
        let long = "x".repeat(MAX_BODY_BYTES + 100);
        for index in 0..(MAX_NOTES + 2) {
            store
                .add(Note::new(format!("q{index}"), long.clone()))
                .unwrap();
        }
        let snapshot = store.snapshot();
        let notes = &snapshot.stacks[0].notes;
        assert_eq!(notes.len(), MAX_NOTES);
        assert_eq!(notes[0].quote, "q2");
        assert_eq!(notes[0].body.len(), MAX_BODY_BYTES);
    }

    #[test]
    fn a_hand_edited_file_is_normalized() {
        let store = temp("hand-edited");
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(
            store.path(),
            serde_json::json!({
                "version": VERSION,
                "active": 9,
                "stacks": [
                    {"name": "  ", "notes": [{"quote": "  ", "body": "hello"}]},
                    {"name": "Only two"},
                ]
            })
            .to_string(),
        )
        .unwrap();
        let snapshot = store.snapshot();
        assert_eq!(snapshot.active, STACK_COUNT - 1);
        assert_eq!(snapshot.stacks.len(), STACK_COUNT);
        assert_eq!(snapshot.stacks[0].name, DEFAULT_NAMES[0]);
        assert_eq!(snapshot.stacks[0].notes, [Note::new("", "hello")]);
        assert_eq!(snapshot.stacks[1].name, "Only two");
    }

    #[test]
    fn a_malformed_file_is_preserved_on_the_next_change() {
        let store = temp("corrupt");
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(store.path(), b"{ not json").unwrap();
        store.add(Note::new("Still", "Works")).unwrap();
        assert!(store.path().with_extension("json.corrupt").exists());
        assert_eq!(store.snapshot().stacks[0].notes.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn the_store_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let store = temp("owner-only");
        store.add(Note::new("A", "B")).unwrap();
        let mode = fs::metadata(store.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let directory = store.path().parent().unwrap();
        let directory_mode = fs::metadata(directory).unwrap().permissions().mode() & 0o777;
        assert_eq!(directory_mode, 0o700);
    }
}
