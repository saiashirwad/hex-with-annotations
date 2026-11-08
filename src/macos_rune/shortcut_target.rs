//! The shortcuts the window records and the listener follows.

/// A shortcut the listener registers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ShortcutTarget {
    #[default]
    Dictation,
    PasteLast,
    #[cfg(hex_capture)]
    Capture,
    /// Labs › Annotate 15 s replay shortcut.
    #[cfg(hex_capture)]
    CaptureReplay,
    RawDictation,
    /// Labs › Second dictation shortcut.
    SecondDictation,
    /// Toggle mode's cancel shortcut.
    Cancel,
    /// Labs › Screenshots.
    Screenshot,
    /// Labs › Stacks' note shortcut.
    StackNote,
    /// Labs › Stacks' paste shortcut.
    StackPaste,
}
