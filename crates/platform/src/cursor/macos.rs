//! The character before the cursor on macOS, over the Accessibility API.
//!
//! What it reads, in order, stopping at the first answer:
//!
//! 1. Secure Input on, or no Accessibility grant: nothing is read.
//! 2. The system-wide element's `AXFocusedUIElement`.
//! 3. That element's `AXSubrole`: an `AXSecureTextField` (a password field)
//!    is never read. Neither is a field whose subrole can't be read (the app
//!    timed out, failed, or answered with something that isn't a string):
//!    it fails closed (#135). Only an element that has no subrole at all
//!    (`kAXErrorAttributeUnsupported` or `kAXErrorNoValue`, as TextEdit's
//!    text view answers) is read on: a password field always has one.
//! 4. Its `AXSelectedTextRange` (UTF-16 offsets): at offset 0 the cursor is
//!    at the start of the field.
//! 5. The two UTF-16 units before the cursor (two, so a character outside
//!    the Basic Multilingual Plane is read whole) through the parameterized
//!    `AXStringForRange`, which asks the app for just that range. An app
//!    that doesn't support it reads as unknown (no space): the field's whole
//!    value (`AXValue`) is never requested, and [`copy_attribute`] refuses
//!    any attribute outside [`READABLE_ATTRIBUTES`]. The text itself is
//!    never logged.
//!
//! Every element gets a messaging timeout of [`AX_TIMEOUT`], so a hung app
//! costs at most that per call; the pipeline also bounds the whole read.

use std::ffi::c_void;
use std::time::Duration;

use dictation_ports::{BeforeCursor, CursorReadError};

use crate::insert::Caret;
use crate::permissions::macos::is_accessibility_trusted;
use crate::secure_input::is_secure_input_enabled;

/// How long each Accessibility call waits for the focused app.
pub const AX_TIMEOUT: Duration = Duration::from_millis(100);

/// UTF-16 units read before the cursor: one character, even a surrogate
/// pair.
const UNITS_BEFORE: isize = 2;

/// The only attributes [`copy_attribute`] asks an app for. None of them is
/// the field's text: that is read only through the bounded
/// `AXStringForRange`.
const READABLE_ATTRIBUTES: [&str; 3] = ["AXFocusedUIElement", "AXSubrole", "AXSelectedTextRange"];

/// `NSAccessibilitySecureTextFieldSubrole`: a password field.
const SECURE_SUBROLE: &str = "AXSecureTextField";

type CfTypeRef = *const c_void;
type AxError = i32;

const AX_SUCCESS: AxError = 0;
const AX_ERROR_INVALID_UI_ELEMENT: AxError = -25202;
const AX_ERROR_CANNOT_COMPLETE: AxError = -25204;
const AX_ERROR_ATTRIBUTE_UNSUPPORTED: AxError = -25205;
const AX_ERROR_NOT_IMPLEMENTED: AxError = -25208;
const AX_ERROR_API_DISABLED: AxError = -25211;
const AX_ERROR_NO_VALUE: AxError = -25212;
const AX_ERROR_PARAMETERIZED_ATTRIBUTE_UNSUPPORTED: AxError = -25213;

/// `kAXValueCFRangeType`.
const AX_VALUE_CF_RANGE_TYPE: u32 = 4;
/// `kCFStringEncodingUTF8`.
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

/// `CFRange`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CfRange {
    location: isize,
    length: isize,
}

// SAFETY: these declarations match the C signatures in ApplicationServices
// (HIServices: AXUIElement.h, AXValue.h); each call site's `unsafe` block
// upholds its contract.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> CfTypeRef;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementSetMessagingTimeout(element: CfTypeRef, timeout_seconds: f32) -> AxError;
    fn AXUIElementCopyAttributeValue(
        element: CfTypeRef,
        attribute: CfTypeRef,
        value: *mut CfTypeRef,
    ) -> AxError;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: CfTypeRef,
        attribute: CfTypeRef,
        parameter: CfTypeRef,
        result: *mut CfTypeRef,
    ) -> AxError;
    fn AXValueCreate(value_type: u32, value: *const c_void) -> CfTypeRef;
    fn AXValueGetTypeID() -> usize;
    fn AXValueGetType(value: CfTypeRef) -> u32;
    fn AXValueGetValue(value: CfTypeRef, value_type: u32, out: *mut c_void) -> u8;
}

// SAFETY: these declarations match the C signatures in CoreFoundation
// (CFBase.h, CFString.h); each call site's `unsafe` block upholds its
// contract.
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: CfTypeRef);
    fn CFGetTypeID(cf: CfTypeRef) -> usize;
    fn CFHash(cf: CfTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringCreateWithBytes(
        allocator: CfTypeRef,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        is_external_representation: u8,
    ) -> CfTypeRef;
    fn CFStringGetLength(string: CfTypeRef) -> isize;
    fn CFStringGetCharacters(string: CfTypeRef, range: CfRange, buffer: *mut u16);
}

/// A Core Foundation object we own (Create or Copy rule), released when
/// dropped. Never null.
struct Cf(CfTypeRef);

impl Cf {
    /// Takes ownership of `object`; `None` if it's null. Lazy: a null is
    /// never wrapped, so it is never released.
    fn owned(object: CfTypeRef) -> Option<Self> {
        if object.is_null() {
            None
        } else {
            Some(Self(object))
        }
    }

    fn type_id(&self) -> usize {
        // SAFETY: self.0 is a valid, retained CF object.
        unsafe { CFGetTypeID(self.0) }
    }

    fn is_string(&self) -> bool {
        // SAFETY: no arguments.
        self.type_id() == unsafe { CFStringGetTypeID() }
    }

    fn is_element(&self) -> bool {
        // SAFETY: no arguments.
        self.type_id() == unsafe { AXUIElementGetTypeID() }
    }

    fn is_ax_value(&self) -> bool {
        // SAFETY: no arguments.
        self.type_id() == unsafe { AXValueGetTypeID() }
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        // SAFETY: we own this reference (Create/Copy rule) and release it
        // exactly once.
        unsafe { CFRelease(self.0) };
    }
}

/// A `CFString` holding `text`.
fn cf_string(text: &str) -> Option<Cf> {
    let length = isize::try_from(text.len()).ok()?;
    // SAFETY: bytes points at `length` valid UTF-8 bytes for the call; the
    // result follows the Create rule.
    Cf::owned(unsafe {
        CFStringCreateWithBytes(
            std::ptr::null(),
            text.as_ptr(),
            length,
            CF_STRING_ENCODING_UTF8,
            0,
        )
    })
}

/// What an Accessibility error means for the reader.
fn ax_error(code: AxError) -> CursorReadError {
    match code {
        AX_ERROR_INVALID_UI_ELEMENT
        | AX_ERROR_ATTRIBUTE_UNSUPPORTED
        | AX_ERROR_PARAMETERIZED_ATTRIBUTE_UNSUPPORTED
        | AX_ERROR_NOT_IMPLEMENTED
        | AX_ERROR_NO_VALUE => CursorReadError::Unavailable,
        // What a messaging timeout (or an app that died) returns.
        AX_ERROR_CANNOT_COMPLETE => CursorReadError::TimedOut,
        AX_ERROR_API_DISABLED => CursorReadError::PermissionDenied,
        other => CursorReadError::Failed(format!("AXError {other}")),
    }
}

/// The element's `attribute`, one of [`READABLE_ATTRIBUTES`]; any other
/// is refused before the app is asked.
fn copy_attribute(element: &Cf, attribute: &str) -> Result<Cf, CursorReadError> {
    copy_attribute_or_code(element, attribute)?.map_err(ax_error)
}

/// [`copy_attribute`], with the app's `AXError` kept as it is (inner `Err`),
/// for a caller that tells the codes apart. The outer `Err` is a refusal
/// before the app is asked. A success with no object reads as
/// `kAXErrorNoValue`.
fn copy_attribute_or_code(
    element: &Cf,
    attribute: &str,
) -> Result<Result<Cf, AxError>, CursorReadError> {
    if !READABLE_ATTRIBUTES.contains(&attribute) {
        return Err(CursorReadError::Failed(format!("{attribute} is not read")));
    }
    let name = cf_string(attribute).ok_or(CursorReadError::Failed("CFString".into()))?;
    let mut value: CfTypeRef = std::ptr::null();
    // SAFETY: element and name are valid CF objects for the call; value is
    // a local out pointer, which on success holds a retained object (Copy
    // rule) that Cf takes over.
    let code = unsafe { AXUIElementCopyAttributeValue(element.0, name.0, &raw mut value) };
    if code != AX_SUCCESS {
        return Ok(Err(code));
    }
    Ok(Cf::owned(value).ok_or(AX_ERROR_NO_VALUE))
}

/// What an `AXSubrole` read that failed with `code` means: the element has
/// no subrole (`Ok(None)`, so it isn't a password field: those always have
/// one), or the subrole is unknown and the field must not be read (#135).
fn subrole_error(code: AxError) -> Result<Option<Vec<u16>>, CursorReadError> {
    match code {
        AX_ERROR_ATTRIBUTE_UNSUPPORTED | AX_ERROR_NO_VALUE => Ok(None),
        other => Err(ax_error(other)),
    }
}

/// The element's `AXStringForRange` for `range`.
fn copy_string_for_range(element: &Cf, range: CfRange) -> Result<Cf, CursorReadError> {
    let name = cf_string("AXStringForRange").ok_or(CursorReadError::Failed("CFString".into()))?;
    // SAFETY: range is a CFRange, as kAXValueCFRangeType says; AXValueCreate
    // copies it and the result follows the Create rule.
    let parameter =
        Cf::owned(unsafe { AXValueCreate(AX_VALUE_CF_RANGE_TYPE, (&raw const range).cast()) })
            .ok_or(CursorReadError::Failed("AXValueCreate".into()))?;
    let mut value: CfTypeRef = std::ptr::null();
    // SAFETY: element, name and parameter are valid CF objects for the
    // call; on success value holds a retained object (Copy rule).
    let code = unsafe {
        AXUIElementCopyParameterizedAttributeValue(element.0, name.0, parameter.0, &raw mut value)
    };
    if code != AX_SUCCESS {
        return Err(ax_error(code));
    }
    Cf::owned(value).ok_or(CursorReadError::Unavailable)
}

/// Sets the per-element messaging timeout; a failure only means the
/// default (about six seconds) applies, which the pipeline's own bound
/// still cuts short.
fn set_timeout(element: &Cf) {
    // SAFETY: element is a valid AXUIElement.
    let _ = unsafe { AXUIElementSetMessagingTimeout(element.0, AX_TIMEOUT.as_secs_f32()) };
}

/// The UTF-16 units of `string` in `range`, which must lie inside it.
fn units(string: &Cf, range: CfRange) -> Option<Vec<u16>> {
    if !string.is_string() {
        return None;
    }
    // SAFETY: string is a valid CFString.
    let length = unsafe { CFStringGetLength(string.0) };
    if range.location < 0 || range.length < 0 || range.location + range.length > length {
        return None;
    }
    let mut buffer = vec![0u16; usize::try_from(range.length).ok()?];
    // SAFETY: range lies inside the string (checked above) and buffer holds
    // exactly range.length units.
    unsafe { CFStringGetCharacters(string.0, range, buffer.as_mut_ptr()) };
    Some(buffer)
}

/// The range of up to [`UNITS_BEFORE`] units that ends at `cursor`.
fn window_before(cursor: isize) -> CfRange {
    let start = (cursor - UNITS_BEFORE).max(0);
    CfRange {
        location: start,
        length: cursor - start,
    }
}

/// The last character of `units` (UTF-16); `None` if they are empty or end
/// in half a surrogate pair.
fn last_char(units: &[u16]) -> Option<char> {
    char::decode_utf16(units.iter().copied())
        .last()
        .and_then(Result::ok)
}

/// What the reader needs from the focused element. The only way to its
/// text is [`string_for_range`](Self::string_for_range), bounded at the
/// source, so nothing here can request the whole field.
trait FocusedField {
    /// Its `AXSubrole` (UTF-16), `None` if it has none, or why it couldn't
    /// be read.
    fn subrole(&self) -> Result<Option<Vec<u16>>, CursorReadError>;
    /// The cursor's offset: the start of `AXSelectedTextRange`.
    fn cursor(&self) -> Result<isize, CursorReadError>;
    /// At most `range.length` UTF-16 units of `AXStringForRange` for
    /// `range`.
    fn string_for_range(&self, range: CfRange) -> Result<Vec<u16>, CursorReadError>;
}

/// The focused element, over Accessibility.
struct AxField(Cf);

impl AxField {
    /// Its `AXSelectedTextRange`: the selection's start and length.
    fn selection(&self) -> Result<CfRange, CursorReadError> {
        let value = copy_attribute(&self.0, "AXSelectedTextRange")?;
        // SAFETY: value is a valid AXValue (checked first).
        if !value.is_ax_value() || unsafe { AXValueGetType(value.0) } != AX_VALUE_CF_RANGE_TYPE {
            return Err(CursorReadError::Unavailable);
        }
        let mut range = CfRange::default();
        // SAFETY: the value holds a CFRange (its type, checked above), and
        // range is a local CFRange to copy it into.
        let ok =
            unsafe { AXValueGetValue(value.0, AX_VALUE_CF_RANGE_TYPE, (&raw mut range).cast()) };
        if ok == 0 || range.location < 0 || range.length < 0 {
            return Err(CursorReadError::Unavailable);
        }
        Ok(range)
    }
}

impl FocusedField for AxField {
    fn subrole(&self) -> Result<Option<Vec<u16>>, CursorReadError> {
        let subrole = match copy_attribute_or_code(&self.0, "AXSubrole")? {
            Ok(subrole) => subrole,
            Err(code) => return subrole_error(code),
        };
        if !subrole.is_string() {
            return Err(CursorReadError::Unavailable);
        }
        // SAFETY: a valid CFString (checked above).
        let length = unsafe { CFStringGetLength(subrole.0) };
        units(
            &subrole,
            CfRange {
                location: 0,
                length,
            },
        )
        .map(Some)
        .ok_or(CursorReadError::Unavailable)
    }

    fn cursor(&self) -> Result<isize, CursorReadError> {
        self.selection().map(|range| range.location)
    }

    fn string_for_range(&self, range: CfRange) -> Result<Vec<u16>, CursorReadError> {
        let string = copy_string_for_range(&self.0, range)?;
        if !string.is_string() {
            return Err(CursorReadError::Unavailable);
        }
        // Some apps return more than asked for; copy out only the end.
        // SAFETY: a valid CFString (checked above).
        let length = unsafe { CFStringGetLength(string.0) };
        let take = length.min(range.length);
        units(
            &string,
            CfRange {
                location: length - take,
                length: take,
            },
        )
        .ok_or(CursorReadError::Unavailable)
    }
}

/// The character before the cursor in `field`: nothing is read from a
/// password field, nor from a field whose subrole can't be read (fail
/// closed, #135), and if the bounded range read fails the answer is unknown
/// (the caller adds no space); there is no other way to the text.
fn read_field(field: &impl FocusedField) -> Result<BeforeCursor, CursorReadError> {
    if field
        .subrole()?
        .is_some_and(|subrole| subrole.iter().copied().eq(SECURE_SUBROLE.encode_utf16()))
    {
        return Err(CursorReadError::SecureField);
    }
    let cursor = field.cursor()?;
    if cursor == 0 {
        return Ok(BeforeCursor::StartOfField);
    }
    let window = window_before(cursor);
    let units = field.string_for_range(window)?;
    let keep = usize::try_from(window.length).unwrap_or(0).min(units.len());
    last_char(&units[units.len() - keep..])
        .map(BeforeCursor::Char)
        .ok_or(CursorReadError::Unavailable)
}

/// The focused element, after the same checks as [`char_before_cursor`]
/// (Secure Input, Accessibility), with a messaging timeout.
fn focused_field() -> Result<AxField, CursorReadError> {
    if is_secure_input_enabled() {
        return Err(CursorReadError::SecureField);
    }
    if !is_accessibility_trusted() {
        return Err(CursorReadError::PermissionDenied);
    }
    // SAFETY: no arguments; the result follows the Create rule.
    let system = Cf::owned(unsafe { AXUIElementCreateSystemWide() })
        .ok_or(CursorReadError::Failed("no system-wide element".into()))?;
    set_timeout(&system);
    let focused = copy_attribute(&system, "AXFocusedUIElement")?;
    if !focused.is_element() {
        return Err(CursorReadError::Unavailable);
    }
    set_timeout(&focused);
    Ok(AxField(focused))
}

/// The caret for the paste's landing check (`crate::cursor::caret`): the
/// selection, and the character before it through [`read_field`], so a
/// password field is never read and at most two units of text are asked
/// for. `None` if any of it can't be read.
pub(super) fn caret() -> Option<Caret> {
    let field = focused_field().ok()?;
    let last = read_field(&field).ok()?.preceding();
    let range = field.selection().ok()?;
    // SAFETY: a valid AXUIElement (focused_field checked its type).
    let hash = unsafe { CFHash(field.0.0) };
    Some(Caret {
        field: u64::try_from(hash).ok()?,
        location: usize::try_from(range.location).ok()?,
        length: usize::try_from(range.length).ok()?,
        last,
    })
}

/// See the module docs.
pub(super) fn char_before_cursor() -> Result<BeforeCursor, CursorReadError> {
    read_field(&focused_field()?)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// A focused field that answers from `text` (UTF-16) and records every
    /// request.
    struct RecordingField {
        text: Vec<u16>,
        cursor: Result<isize, CursorReadError>,
        /// What `AXSubrole` answers; a plain text area has none.
        subrole: Result<Option<&'static str>, CursorReadError>,
        range_read: Option<CursorReadError>,
        /// Extra units an app returns around the asked-for range.
        overshoot: bool,
        requests: RefCell<Vec<String>>,
    }

    impl RecordingField {
        fn new(text: &str, cursor: isize) -> Self {
            Self {
                text: text.encode_utf16().collect(),
                cursor: Ok(cursor),
                subrole: Ok(None),
                range_read: None,
                overshoot: false,
                requests: RefCell::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<String> {
            self.requests.borrow().clone()
        }
    }

    impl FocusedField for RecordingField {
        fn subrole(&self) -> Result<Option<Vec<u16>>, CursorReadError> {
            self.requests.borrow_mut().push("AXSubrole".into());
            self.subrole
                .clone()
                .map(|subrole| subrole.map(|s| s.encode_utf16().collect()))
        }

        fn cursor(&self) -> Result<isize, CursorReadError> {
            self.requests
                .borrow_mut()
                .push("AXSelectedTextRange".into());
            self.cursor.clone()
        }

        fn string_for_range(&self, range: CfRange) -> Result<Vec<u16>, CursorReadError> {
            self.requests.borrow_mut().push(format!(
                "AXStringForRange {}+{}",
                range.location, range.length
            ));
            if let Some(error) = &self.range_read {
                return Err(error.clone());
            }
            let start = usize::try_from(range.location).unwrap();
            let end = start + usize::try_from(range.length).unwrap();
            if self.overshoot {
                return Ok(self.text.clone());
            }
            Ok(self.text[start..end].to_vec())
        }
    }

    #[test]
    fn reads_only_the_two_units_before_the_cursor() {
        let field = RecordingField::new("hello world", 5);
        assert_eq!(read_field(&field), Ok(BeforeCursor::Char('o')));
        assert_eq!(
            field.requests(),
            ["AXSubrole", "AXSelectedTextRange", "AXStringForRange 3+2"]
        );
    }

    /// B1 (review round 1): an app without AXStringForRange reads as
    /// unknown, and nothing else (never AXValue, the whole field) is asked.
    #[test]
    fn an_unsupported_range_read_is_unknown_and_never_asks_for_the_whole_field() {
        for error in [
            CursorReadError::Unavailable,
            CursorReadError::Failed("AXError -25200".into()),
            CursorReadError::TimedOut,
        ] {
            let mut field = RecordingField::new(&"x".repeat(1_000_000), 999_999);
            field.range_read = Some(error.clone());
            assert_eq!(read_field(&field), Err(error));
            let requests = field.requests();
            assert_eq!(
                requests,
                [
                    "AXSubrole",
                    "AXSelectedTextRange",
                    "AXStringForRange 999997+2"
                ]
            );
            assert!(!requests.iter().any(|r| r.contains("AXValue")));
        }
    }

    #[test]
    fn copy_attribute_refuses_the_fields_text_before_asking_the_app() {
        // The system-wide element: creating it asks no app anything, and the
        // refusal comes before any call that would.
        // SAFETY: no arguments; the result follows the Create rule and Cf
        // releases it.
        let system = Cf::owned(unsafe { AXUIElementCreateSystemWide() }).unwrap();
        for attribute in ["AXValue", "AXSelectedText", "AXNumberOfCharacters"] {
            assert_eq!(
                copy_attribute(&system, attribute).err(),
                Some(CursorReadError::Failed(format!("{attribute} is not read"))),
                "{attribute}"
            );
        }
        assert!(!READABLE_ATTRIBUTES.contains(&"AXValue"));
    }

    #[test]
    fn a_password_field_is_never_read() {
        let mut field = RecordingField::new("secret", 6);
        field.subrole = Ok(Some("AXSecureTextField"));
        assert_eq!(read_field(&field), Err(CursorReadError::SecureField));
        assert_eq!(field.requests(), ["AXSubrole"]);
    }

    /// #135: a subrole that can't be read fails closed: nothing more is
    /// asked of the field (no cursor, no text), so no space is added.
    #[test]
    fn a_field_whose_subrole_cant_be_read_is_never_read() {
        for error in [
            CursorReadError::TimedOut,
            CursorReadError::Unavailable,
            CursorReadError::PermissionDenied,
            CursorReadError::Failed("AXError -25200".into()),
        ] {
            let mut field = RecordingField::new("secret", 6);
            field.subrole = Err(error.clone());
            assert_eq!(read_field(&field), Err(error));
            assert_eq!(field.requests(), ["AXSubrole"]);
        }
    }

    #[test]
    fn a_field_with_another_subrole_or_none_is_read() {
        for subrole in [None, Some("AXSearchField"), Some("AXSecureTextFieldX")] {
            let mut field = RecordingField::new("hello", 5);
            field.subrole = Ok(subrole);
            assert_eq!(
                read_field(&field),
                Ok(BeforeCursor::Char('o')),
                "{subrole:?}"
            );
        }
    }

    /// #135: only "the element has no subrole" reads on; every other
    /// AXError on `AXSubrole` (a timeout, a dead or invalid element, an
    /// unknown failure) means the subrole is unknown.
    #[test]
    fn only_a_missing_subrole_counts_as_not_a_password_field() {
        assert_eq!(subrole_error(AX_ERROR_ATTRIBUTE_UNSUPPORTED), Ok(None));
        assert_eq!(subrole_error(AX_ERROR_NO_VALUE), Ok(None));
        assert_eq!(
            subrole_error(AX_ERROR_CANNOT_COMPLETE),
            Err(CursorReadError::TimedOut)
        );
        for code in [
            AX_ERROR_INVALID_UI_ELEMENT,
            AX_ERROR_NOT_IMPLEMENTED,
            AX_ERROR_PARAMETERIZED_ATTRIBUTE_UNSUPPORTED,
        ] {
            assert_eq!(subrole_error(code), Err(CursorReadError::Unavailable));
        }
        assert_eq!(
            subrole_error(AX_ERROR_API_DISABLED),
            Err(CursorReadError::PermissionDenied)
        );
        assert_eq!(
            subrole_error(-25200),
            Err(CursorReadError::Failed("AXError -25200".into()))
        );
    }

    #[test]
    fn the_start_of_a_field_reads_no_text() {
        let field = RecordingField::new("hello", 0);
        assert_eq!(read_field(&field), Ok(BeforeCursor::StartOfField));
        assert_eq!(field.requests(), ["AXSubrole", "AXSelectedTextRange"]);
    }

    #[test]
    fn a_surrogate_pair_before_the_cursor_is_read_whole() {
        let field = RecordingField::new("a\u{1F600}", 3);
        assert_eq!(read_field(&field), Ok(BeforeCursor::Char('\u{1F600}')));
    }

    #[test]
    fn extra_units_an_app_returns_are_cut_to_the_window() {
        let mut field = RecordingField::new("abc", 1);
        field.overshoot = true;
        // Asked for 0+1, got "abc": only the last asked-for unit counts.
        assert_eq!(read_field(&field), Ok(BeforeCursor::Char('c')));
    }

    #[test]
    fn no_cursor_is_unknown() {
        let mut field = RecordingField::new("hello", 0);
        field.cursor = Err(CursorReadError::Unavailable);
        assert_eq!(read_field(&field), Err(CursorReadError::Unavailable));
    }

    /// M1 (review round 1): a null result is never wrapped, so it is never
    /// released (CFRelease(NULL) traps).
    #[test]
    fn a_null_object_is_not_owned_and_never_released() {
        assert!(Cf::owned(std::ptr::null()).is_none());
    }

    #[test]
    fn the_window_is_two_units_before_the_cursor_clamped_at_the_start() {
        assert_eq!(
            window_before(10),
            CfRange {
                location: 8,
                length: 2
            }
        );
        assert_eq!(
            window_before(1),
            CfRange {
                location: 0,
                length: 1
            }
        );
    }

    #[test]
    fn the_last_character_is_decoded_whole() {
        assert_eq!(last_char(&[u16::from(b'a'), u16::from(b'b')]), Some('b'));
        assert_eq!(last_char(&[u16::from(b' ')]), Some(' '));
        // U+1F600 as a surrogate pair.
        assert_eq!(last_char(&[0xD83D, 0xDE00]), Some('\u{1F600}'));
        // Only the low half made it into the window: unknown.
        assert_eq!(last_char(&[0xDE00]), None);
        assert_eq!(last_char(&[]), None);
    }

    #[test]
    fn accessibility_errors_map_to_reasons() {
        assert_eq!(ax_error(AX_ERROR_NO_VALUE), CursorReadError::Unavailable);
        assert_eq!(
            ax_error(AX_ERROR_ATTRIBUTE_UNSUPPORTED),
            CursorReadError::Unavailable
        );
        assert_eq!(
            ax_error(AX_ERROR_CANNOT_COMPLETE),
            CursorReadError::TimedOut
        );
        assert_eq!(
            ax_error(AX_ERROR_API_DISABLED),
            CursorReadError::PermissionDenied
        );
        assert_eq!(
            ax_error(-25200),
            CursorReadError::Failed("AXError -25200".into())
        );
    }

    #[test]
    fn units_are_read_only_inside_the_string() {
        let string = cf_string("héllo").unwrap();
        assert_eq!(
            units(
                &string,
                CfRange {
                    location: 3,
                    length: 2
                }
            ),
            Some(vec![u16::from(b'l'), u16::from(b'o')])
        );
        assert_eq!(
            units(
                &string,
                CfRange {
                    location: 4,
                    length: 2
                }
            ),
            None
        );
        assert!(cf_string("x").unwrap().is_string());
    }
}
