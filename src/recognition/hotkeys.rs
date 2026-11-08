//! The dictation shortcut machines: dictation, Labs' second dictation
//! shortcut, dictate without processing, Labs' Stacks note shortcut, and Caps
//! Lock's hands-free toggle. One records at a time, and the one that started
//! a capture finishes or cancels it.

use crate::audio::CaptureInstant;
use crate::parakeet::TranscriptionTarget;
use crate::suppression::{DictationHotkey, HotkeyAction, InputEvent};

/// What a finished shortcut capture does with its transcript: paste it as
/// usual, paste it without Output processing, or add it to the active stack
/// as a note (Labs › Stacks).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum CaptureTarget {
    #[default]
    Paste,
    RawPaste,
    StackNote,
}

impl CaptureTarget {
    pub(super) const fn transcription(self) -> TranscriptionTarget {
        match self {
            Self::Paste => TranscriptionTarget::Paste,
            Self::RawPaste => TranscriptionTarget::RawPaste,
            Self::StackNote => TranscriptionTarget::StackNote,
        }
    }
}

/// One of the dictation shortcut machines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Shortcut {
    Dictation,
    /// Labs › Second dictation shortcut: dictates as the dictation shortcut
    /// does.
    Second,
    /// Dictate without processing.
    Raw,
    /// Labs › Stacks: note the front app's selection.
    StackNote,
    /// Labs › Caps Lock dictation: a press toggles a hands-free capture.
    CapsLock,
}

impl Shortcut {
    const ALL: [Self; 5] = [
        Self::Dictation,
        Self::Second,
        Self::Raw,
        Self::StackNote,
        Self::CapsLock,
    ];
}

/// The dictation shortcut machines: dictation, Labs › Second dictation
/// shortcut while it is on and bound, Dictate without processing while it is
/// bound, Labs › Stacks' note shortcut while its lab is on and it is bound,
/// and Caps Lock's hands-free toggle. One records at a time, and the one
/// that started a capture finishes or cancels it.
pub(super) struct Hotkeys {
    pub(super) dictation: DictationHotkey,
    pub(super) second: Option<DictationHotkey>,
    pub(super) raw: Option<DictationHotkey>,
    /// Labs › Stacks' note shortcut while the lab is on and it is bound.
    pub(super) stack_note: Option<DictationHotkey>,
    /// Labs › Caps Lock dictation's hands-free toggle. Its binding is empty:
    /// presses arrive from the caps-lock worker, not the shortcut matcher.
    pub(super) caps_lock: DictationHotkey,
    /// What the current capture does with its transcript.
    pub(super) capture_target: CaptureTarget,
    /// The front app's selection when the stack note started, waiting for
    /// its transcript.
    pub(super) stack_quote: Option<String>,
}

impl Hotkeys {
    fn machines(&mut self) -> impl Iterator<Item = &mut DictationHotkey> {
        std::iter::once(&mut self.dictation)
            .chain(self.second.as_mut())
            .chain(self.raw.as_mut())
            .chain(self.stack_note.as_mut())
            .chain(Some(&mut self.caps_lock))
    }

    fn machine(&self, shortcut: Shortcut) -> Option<&DictationHotkey> {
        match shortcut {
            Shortcut::Dictation => Some(&self.dictation),
            Shortcut::Second => self.second.as_ref(),
            Shortcut::Raw => self.raw.as_ref(),
            Shortcut::StackNote => self.stack_note.as_ref(),
            Shortcut::CapsLock => Some(&self.caps_lock),
        }
    }

    fn machine_mut(&mut self, shortcut: Shortcut) -> Option<&mut DictationHotkey> {
        match shortcut {
            Shortcut::Dictation => Some(&mut self.dictation),
            Shortcut::Second => self.second.as_mut(),
            Shortcut::Raw => self.raw.as_mut(),
            Shortcut::StackNote => self.stack_note.as_mut(),
            Shortcut::CapsLock => Some(&mut self.caps_lock),
        }
    }

    /// Whether a machine other than `shortcut`'s records.
    fn recording_besides(&self, shortcut: Shortcut) -> bool {
        Shortcut::ALL
            .into_iter()
            .filter(|other| *other != shortcut)
            .filter_map(|other| self.machine(other))
            .any(DictationHotkey::is_recording)
    }

    /// Whether a machine other than `shortcut`'s has a double-tap gesture
    /// waiting to become a recording.
    fn pending_gesture_besides(&self, shortcut: Shortcut) -> bool {
        Shortcut::ALL
            .into_iter()
            .filter(|other| *other != shortcut)
            .filter_map(|other| self.machine(other))
            .any(DictationHotkey::has_pending_gesture)
    }

    pub(super) fn any_recording(&self) -> bool {
        Shortcut::ALL
            .into_iter()
            .filter_map(|shortcut| self.machine(shortcut))
            .any(DictationHotkey::is_recording)
    }

    pub(super) fn suppresses_recognition(&self) -> bool {
        Shortcut::ALL
            .into_iter()
            .filter_map(|shortcut| self.machine(shortcut))
            .any(DictationHotkey::suppresses_recognition)
    }

    /// Stands `shortcut`'s machine down, as when its start was refused.
    pub(super) fn suspend(&mut self, shortcut: Shortcut) {
        match self.machine_mut(shortcut) {
            Some(machine) => machine.suspend(),
            None => self.dictation.suspend(),
        };
    }

    /// Suspends every machine (never stopping at the first, as `any`
    /// would) and reports whether one was recording.
    pub(super) fn suspend_all(&mut self) -> bool {
        self.machines().filter_map(DictationHotkey::suspend).count() > 0
    }

    pub(super) fn disarm_pending_gestures(&mut self) {
        self.machines()
            .for_each(DictationHotkey::disarm_pending_gesture);
    }

    pub(super) fn track_key_state(&mut self, event: InputEvent, at: CaptureInstant) {
        for machine in self.machines() {
            machine.track_key_state(event, at);
        }
    }

    pub(super) fn recover_stale_keys(&mut self) {
        self.machines()
            .for_each(DictationHotkey::recover_stale_keys);
    }

    pub(super) fn break_chords(&mut self, now: CaptureInstant) {
        for machine in self.machines() {
            machine.break_chord(now);
        }
    }

    /// Applies the saved shortcut options. Unbinding the raw or second
    /// shortcut (or turning the second's Labs toggle off) while it records
    /// reports true: the caller cancels that recording.
    pub(super) fn configure(&mut self, now: CaptureInstant) -> bool {
        let hotkeys = crate::app_settings::runtime_hotkeys();
        let raw_cancelled = rebind(&mut self.raw, hotkeys.raw_dictation, now);
        let second_cancelled = rebind(&mut self.second, hotkeys.second_dictation, now);
        let stack_cancelled = rebind(&mut self.stack_note, hotkeys.stack_note, now);
        self.dictation.set_binding(hotkeys.dictation);
        for machine in self.machines() {
            machine.set_double_tap_enabled(crate::app_settings::double_tap_lock());
            machine.set_double_tap_only(crate::app_settings::double_tap_only());
            machine.set_toggle(crate::app_settings::shortcut_toggle());
            machine.set_cancel(hotkeys.cancel);
        }
        // Caps Lock is hands-free by definition: its own machine always
        // treats a press as Toggle, and Escape cancels it in every mode.
        self.caps_lock
            .set_binding(crate::app_settings::RuntimeHotkey::default());
        self.caps_lock.set_double_tap_enabled(false);
        self.caps_lock.set_double_tap_only(false);
        self.caps_lock.set_toggle(true);
        self.caps_lock.set_cancel(Some(
            crate::app_settings::HotkeyBinding::cancel_default().runtime(),
        ));
        raw_cancelled || second_cancelled || stack_cancelled
    }

    /// A Caps Lock press, if the lab's machine should act on it. A hold
    /// capture, another shortcut recording, or a pending double-tap gesture
    /// is left undisturbed.
    pub(super) fn caps_lock_press(&mut self) -> Option<HotkeyAction> {
        if self.recording_besides(Shortcut::CapsLock)
            || self.pending_gesture_besides(Shortcut::CapsLock)
        {
            return None;
        }
        let action = self.caps_lock.caps_lock_toggle()?;
        if action == HotkeyAction::Start {
            self.capture_target = CaptureTarget::Paste;
            self.stack_quote = None;
        }
        Some(action)
    }

    /// Stands a Caps Lock capture down when its lab turns off. Returns
    /// whether one was recording so the caller can cancel it.
    pub(super) fn stop_caps_lock(&mut self) -> bool {
        if self.caps_lock.is_recording() {
            self.caps_lock.suspend();
            true
        } else {
            false
        }
    }

    /// Feeds one input to every machine, and returns what each did with
    /// which machine it was. A machine that starts while another records
    /// stands down until its keys come up, and only the dictation machine
    /// pastes.
    pub(super) fn process(
        &mut self,
        event: InputEvent,
        at: CaptureInstant,
    ) -> Vec<(Shortcut, HotkeyAction)> {
        let mut actions = Vec::new();
        for shortcut in Shortcut::ALL {
            let others_recording = self.recording_besides(shortcut);
            let Some(machine) = self.machine_mut(shortcut) else {
                continue;
            };
            match machine.process(event, at) {
                Some(HotkeyAction::Start) if others_recording => {
                    machine.suspend();
                }
                Some(
                    HotkeyAction::PasteLast | HotkeyAction::PasteMeeting | HotkeyAction::PasteStack,
                ) if shortcut != Shortcut::Dictation => {}
                Some(action) => actions.push((shortcut, action)),
                None => {}
            }
        }
        actions
    }

    /// Stops whichever machine records, as a `stop` or `cancel` command does.
    pub(super) fn suspend_recording(&mut self) {
        for machine in self.machines() {
            if machine.is_recording() {
                machine.suspend();
            }
        }
    }
}

/// Creates, rebinds, or removes an optional shortcut machine. Returns
/// whether a removed machine was recording.
fn rebind(
    slot: &mut Option<DictationHotkey>,
    binding: Option<crate::app_settings::RuntimeHotkey>,
    now: CaptureInstant,
) -> bool {
    match binding {
        None => slot.take().is_some_and(|machine| machine.is_recording()),
        Some(binding) => {
            match slot.as_mut() {
                Some(machine) => machine.set_binding(binding),
                None => {
                    *slot = Some(DictationHotkey::new(
                        now,
                        crate::app_settings::double_tap_lock(),
                        binding,
                    ));
                }
            }
            false
        }
    }
}
