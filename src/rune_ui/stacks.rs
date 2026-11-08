//! The Stacks pane (macOS): notes collected from what you read. Each stack
//! holds quotes with what you said about them; one is active, and its
//! Markdown is what the paste shortcut sends. The note and paste shortcuts
//! are recorded here, and the listener adds spoken notes to the same file.

use rune::{El, Icon, Tx, col, row, text};

#[cfg(target_os = "macos")]
use super::ShortcutTarget;
use super::Ui;
use super::controls::{ButtonKind, METRICS, Paint, button, page, raised, section};
#[cfg(target_os = "macos")]
use super::general::shortcut_rows;
use crate::desktop::i18n::tr;
use crate::stacks::{Note, Snapshot, Stacks};

/// The sidebar and shortcut rows' mark: notes stacked on one another.
pub(super) static STACKS_ICON: Icon = Icon {
    name: "hex-stacks",
    view: 16.0,
    body: r#"<path d="M3 5.5h10M3 8h10M3 10.5h6" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#,
};

/// A quote's mark in the note rows.
static QUOTE_ICON: Icon = Icon {
    name: "hex-stacks-quote",
    view: 16.0,
    body: r#"<path d="M4 10.5c0-2 .8-3.4 2.4-4.2M9 10.5c0-2 .8-3.4 2.4-4.2M4.6 10.5h1.8v2H4.6zM9.6 10.5h1.8v2H9.6z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

/// The pane's state: the store, the snapshot drawn, and the header's
/// transient answers.
#[derive(Default)]
pub(super) struct StacksPane {
    store: Option<Stacks>,
    snapshot: Snapshot,
    error: Option<String>,
    pub(super) copied: bool,
    pub(super) clear_armed: bool,
}

impl StacksPane {
    /// The store in Hex's data folder. A failure leaves the pane reading
    /// empty stacks and saying why.
    pub(super) fn open() -> Self {
        match Stacks::open_default() {
            Ok(store) => {
                let snapshot = store.snapshot();
                Self {
                    store: Some(store),
                    snapshot,
                    ..Self::default()
                }
            }
            Err(error) => Self {
                error: Some(format!("{error}")),
                ..Self::default()
            },
        }
    }

    /// A scene's stacks: no store behind them, so a snapshot never reads or
    /// writes the person's notes.
    pub(super) fn fixture() -> Self {
        let mut snapshot = Snapshot::default();
        snapshot.stacks[0].notes = vec![
            Note::new(
                "The model loads on the first dictation, not at launch.",
                "So the menu bar icon is the only cold-start sign. Worth saying in the docs.",
            ),
            Note::new(
                "A stack is just Markdown: a quote, then what you said about it.",
                "That makes it easy to paste into a note and hard to lock in.",
            ),
        ];
        Self {
            snapshot,
            ..Self::default()
        }
    }

    /// Reads the file again while the pane shows, so a spoken note lands
    /// without reopening the window. Reports whether it changed.
    pub(super) fn poll(&mut self, visible: bool) -> bool {
        if !visible {
            return false;
        }
        let Some(store) = &self.store else {
            return false;
        };
        let snapshot = store.snapshot();
        if snapshot == self.snapshot {
            return false;
        }
        self.snapshot = snapshot;
        true
    }

    fn use_stack(&mut self, index: usize) {
        let Some(store) = &self.store else {
            return;
        };
        match store.use_stack(index) {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                self.copied = false;
                self.clear_armed = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn clear(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        match store.clear() {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                self.clear_armed = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

/// The header's Copy and Clear actions.
pub(super) fn actions(ui: &Ui, p: Paint) -> Vec<El> {
    let pane = &ui.stacks;
    let stack = pane.snapshot.active_stack();
    let empty = stack.is_none_or(|stack| stack.notes.is_empty());
    let copy = button(
        p,
        "stacks-copy",
        if pane.copied {
            tr("Copied")
        } else {
            tr("Copy all")
        },
        ButtonKind::Secondary,
        |ui: &mut Ui, tx: &mut Tx| {
            let markdown = ui.stacks.snapshot.markdown(ui.stacks.snapshot.active);
            if markdown.is_empty() {
                return;
            }
            tx.copy(markdown);
            ui.stacks.copied = true;
        },
    );
    let clear = if pane.clear_armed {
        button(
            p,
            "stacks-clear",
            tr("Really clear?"),
            ButtonKind::Danger,
            |ui: &mut Ui, _| ui.stacks.clear(),
        )
    } else {
        button(
            p,
            "stacks-clear",
            tr("Clear stack"),
            ButtonKind::Secondary,
            |ui: &mut Ui, _| {
                ui.stacks.clear_armed = true;
                ui.stacks.copied = false;
            },
        )
    };
    let mut actions = vec![copy];
    if !empty {
        actions.push(clear);
    }
    actions
}

/// The pane body: the shortcuts, the four stacks, and the active one's
/// notes.
pub(super) fn pane(ui: &Ui, p: Paint) -> El {
    let pane = &ui.stacks;
    let mut content = page("stacks-page").child(shortcuts(ui, p));
    if let Some(error) = &pane.error {
        content = content.child(
            row()
                .key("stacks-error")
                .px(METRICS.row_px)
                .child(text(error.clone()).font(p.k.ty.ui).color(p.k.c.danger.fg)),
        );
    }
    content = content.child(stacks_card(ui, p));
    content = content.child(notes_card(ui, p));
    super::scroll_body(p.k, "stacks-scroll", content)
}

/// Both shortcuts, recorded the same way General's are.
fn shortcuts(ui: &Ui, p: Paint) -> El {
    #[cfg(target_os = "macos")]
    let rows = {
        let mut rows = shortcut_rows(
            ui,
            p,
            "stack-note",
            &STACKS_ICON,
            tr("Record a note"),
            ShortcutTarget::StackNote,
            Some(tr(
                "Hold, say what you think about the selection, and let go",
            )),
        );
        rows.extend(shortcut_rows(
            ui,
            p,
            "stack-paste",
            &STACKS_ICON,
            tr("Paste the stack"),
            ShortcutTarget::StackPaste,
            Some(tr(
                "Paste the active stack as Markdown into the front app, then clear it",
            )),
        ));
        rows
    };
    #[cfg(not(target_os = "macos"))]
    let rows = Vec::new();
    section(p, "stacks-shortcuts", tr("Shortcuts"), rows)
}

/// The four stacks, one active.
fn stacks_card(ui: &Ui, p: Paint) -> El {
    let pane = &ui.stacks;
    let labels: Vec<String> = pane
        .snapshot
        .stacks
        .iter()
        .map(|stack| format!("{} · {}", stack.name, stack.notes.len()))
        .collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let tabs = shortcut_stack_tabs(p, &labels, pane.snapshot.active);
    section(
        p,
        "stacks-picker",
        tr("Stacks"),
        vec![super::controls::copy_row(
            p,
            "stacks-tabs-row",
            None,
            tr("Active stack"),
            None,
            tabs,
            false,
        )],
    )
}

fn shortcut_stack_tabs(p: Paint, labels: &[&str], active: usize) -> El {
    super::controls::segmented(p, "stacks-tabs", labels, active, |ui: &mut Ui, index| {
        ui.stacks.use_stack(index)
    })
}

/// The active stack's notes, newest last, as one card.
fn notes_card(ui: &Ui, p: Paint) -> El {
    let pane = &ui.stacks;
    let Some(stack) = pane.snapshot.active_stack() else {
        return raised(p, "stacks-notes");
    };
    let mut card = raised(p, "stacks-notes");
    if stack.notes.is_empty() {
        card = card.child(
            col()
                .key("stacks-empty")
                .gap(4.0)
                .px(METRICS.row_px)
                .py(16.0)
                .child(
                    text(tr("No notes yet"))
                        .key("title")
                        .font(p.k.ty.ui_medium)
                        .color(p.text()),
                )
                .child(
                    text(tr(
                        "Select text anywhere, hold the note shortcut, and say what you think about it",
                    ))
                    .key("hint")
                    .font(p.k.ty.ui)
                    .color(p.muted()),
                ),
        );
        return card;
    }
    for (index, note) in stack.notes.iter().enumerate() {
        if index > 0 {
            card = card.child(
                rune::el()
                    .key(format!("stacks-divider-{index}"))
                    .h(p.hair)
                    .ml(METRICS.row_px)
                    .mr(METRICS.row_px)
                    .fill(p.k.c.border_muted)
                    .a11y_hidden(),
            );
        }
        card = card.child(note_row(p, index, note));
    }
    card
}

fn note_row(p: Paint, index: usize, note: &Note) -> El {
    let mut note_row = col()
        .key(format!("stacks-note-{index}"))
        .role("listitem")
        .gap(6.0)
        .px(METRICS.row_px)
        .py(12.0);
    if !note.quote.is_empty() {
        note_row = note_row.child(
            row()
                .key("quote")
                .gap(6.0)
                .fill(p.k.c.layer_02)
                .radius(p.k.radius.sm)
                .px(8.0)
                .py(6.0)
                .child(rune::icon(&QUOTE_ICON, 12.0).color(p.k.c.icon_muted))
                .child(
                    text(note.quote.clone())
                        .key("text")
                        .font(p.k.ty.ui)
                        .color(p.muted()),
                ),
        );
    }
    if !note.body.is_empty() {
        note_row = note_row.child(
            text(note.body.clone())
                .key("body")
                .font(p.k.ty.ui)
                .color(p.text()),
        );
    }
    note_row
}
