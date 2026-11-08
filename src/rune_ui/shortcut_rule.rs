//! The OS's rules for a shortcut, the ones Settings applies when recording
//! one; shared with `hex settings set`. Kept out of `rune_ui.rs` so its
//! source stays under the file cap.

use super::host;
use super::host::ShortcutTarget;
use crate::app_settings::{HotkeyBinding, HotkeySlot};

pub(crate) fn shortcut_rule(slot: HotkeySlot, binding: &HotkeyBinding) -> Result<(), &'static str> {
    let target = match slot {
        HotkeySlot::Dictation => ShortcutTarget::Dictation,
        HotkeySlot::PasteLast => ShortcutTarget::PasteLast,
        HotkeySlot::RawDictation => ShortcutTarget::RawDictation,
        HotkeySlot::Cancel => ShortcutTarget::Cancel,
        #[cfg(hex_capture)]
        HotkeySlot::Capture => ShortcutTarget::Capture,
        #[cfg(all(hex_capture, target_os = "macos"))]
        HotkeySlot::CaptureReplay => ShortcutTarget::CaptureReplay,
        #[cfg(all(hex_capture, not(target_os = "macos")))]
        HotkeySlot::CaptureReplay => ShortcutTarget::Capture,
        #[cfg(target_os = "macos")]
        HotkeySlot::SecondDictation => ShortcutTarget::SecondDictation,
        #[cfg(target_os = "macos")]
        HotkeySlot::Screenshot => ShortcutTarget::Screenshot,
        // Labs › Stacks is macOS-only and records like a second dictation hit.
        #[cfg(target_os = "macos")]
        HotkeySlot::StackNote | HotkeySlot::StackPaste => ShortcutTarget::SecondDictation,
        // Transform typed text is Linux-only. A saved binding meets the
        // dictation shortcut's rules everywhere else, including macOS, where
        // it is never registered.
        HotkeySlot::TypedText => ShortcutTarget::Dictation,
        // Labs › Second dictation shortcut and Screenshots are macOS-only;
        // elsewhere a saved one meets the dictation shortcut's rules.
        #[cfg(not(target_os = "macos"))]
        HotkeySlot::SecondDictation
        | HotkeySlot::Screenshot
        | HotkeySlot::StackNote
        | HotkeySlot::StackPaste => ShortcutTarget::Dictation,
        // Labs › Terminal paste shortcut: its own hook rules where it exists,
        // and the dictation shortcut's everywhere else, where it never runs.
        #[cfg(target_os = "macos")]
        HotkeySlot::TerminalPaste => ShortcutTarget::SecondDictation,
        #[cfg(not(target_os = "macos"))]
        HotkeySlot::TerminalPaste => ShortcutTarget::RawDictation,
    };
    host::validate_hotkey(target, binding)
}

/// The recorder's key for `target`.
pub(super) fn hotkey_key(target: ShortcutTarget) -> &'static str {
    match target {
        ShortcutTarget::Dictation => "dictation-hotkey",
        ShortcutTarget::PasteLast => "paste-last-hotkey",
        ShortcutTarget::RawDictation => "raw-dictation-hotkey",
        #[cfg(target_os = "linux")]
        ShortcutTarget::TypedText => "typed-text-hotkey",
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        ShortcutTarget::TerminalPaste => "terminal-paste-hotkey",
        ShortcutTarget::Cancel => "cancel-hotkey",
        #[cfg(hex_capture)]
        ShortcutTarget::Capture => "capture-hotkey",
        #[cfg(all(hex_capture, target_os = "macos"))]
        ShortcutTarget::CaptureReplay => "capture-replay-hotkey",
        #[cfg(target_os = "macos")]
        ShortcutTarget::SecondDictation => "second-dictation-hotkey",
        #[cfg(target_os = "macos")]
        ShortcutTarget::Screenshot => "screenshot-hotkey",
        #[cfg(target_os = "macos")]
        ShortcutTarget::StackNote => "stack-note-hotkey",
        #[cfg(target_os = "macos")]
        ShortcutTarget::StackPaste => "stack-paste-hotkey",
    }
}
