//! Misc functions to improve readability

use super::{Error, StaticString};
use core::ptr::copy;

pub(crate) trait IntoLossy<T>: Sized {
  fn into_lossy(self) -> T;
}

/// Marks branch as impossible, UB if taken in prod, panics in debug
///
/// This function should never be used lightly, it will cause UB if used wrong
#[inline]
#[allow(unused_variables)]
pub(crate) unsafe fn never(s: &str) -> ! {
  #[cfg(debug_assertions)]
  panic!("{}", s);

  #[cfg(not(debug_assertions))]
  core::hint::unreachable_unchecked()
}

/// Encodes `char` into `StaticString` at specified position, heavily unsafe
///
/// We reimplement the `core` function to avoid panicking (UB instead, be careful)
///
/// Reimplemented from:
///
/// `https://github.com/rust-lang/rust/blob/7843e2792dce0f20d23b3c1cca51652013bef0ea/src/libcore/char/methods.rs#L447`
/// # Safety
///
/// - It's UB if index is outside of buffer's boundaries (buffer needs at most 4 bytes)
/// - It's UB if index is inside a character (like a index 3 for "a🤔")
#[inline]
pub(crate) unsafe fn encode_char_utf8_unchecked<const N: usize>(
  s: &mut StaticString<N>,
  ch: char,
  index: usize,
)
{
  // UTF-8 ranges and tags for encoding characters
  #[allow(clippy::missing_docs_in_private_items)]
  const TAG_CONT: u8 = 0b1000_0000;
  #[allow(clippy::missing_docs_in_private_items)]
  const TAG_TWO_B: u8 = 0b1100_0000;
  #[allow(clippy::missing_docs_in_private_items)]
  const TAG_THREE_B: u8 = 0b1110_0000;
  #[allow(clippy::missing_docs_in_private_items)]
  const TAG_FOUR_B: u8 = 0b1111_0000;
  #[allow(clippy::missing_docs_in_private_items)]
  const MAX_ONE_B: u32 = 0x80;
  #[allow(clippy::missing_docs_in_private_items)]
  const MAX_TWO_B: u32 = 0x800;
  #[allow(clippy::missing_docs_in_private_items)]
  const MAX_THREE_B: u32 = 0x10000;

  debug_assert!(ch.len_utf8().saturating_add(index) <= s.capacity());
  debug_assert!(ch.len_utf8().saturating_add(s.len()) <= s.capacity());
  let dst = s.as_mut_bytes().get_unchecked_mut(index..);
  let code = ch as u32;

  if code < MAX_ONE_B {
    debug_assert!(!dst.is_empty());
    *dst.get_unchecked_mut(0) = code.into_lossy();
  } else if code < MAX_TWO_B {
    debug_assert!(dst.len() >= 2);
    *dst.get_unchecked_mut(0) = (code >> 6 & 0x1F).into_lossy() | TAG_TWO_B;
    *dst.get_unchecked_mut(1) = (code & 0x3F).into_lossy() | TAG_CONT;
  } else if code < MAX_THREE_B {
    debug_assert!(dst.len() >= 3);
    *dst.get_unchecked_mut(0) = (code >> 12 & 0x0F).into_lossy() | TAG_THREE_B;
    *dst.get_unchecked_mut(1) = (code >> 6 & 0x3F).into_lossy() | TAG_CONT;
    *dst.get_unchecked_mut(2) = (code & 0x3F).into_lossy() | TAG_CONT;
  } else {
    debug_assert!(dst.len() >= 4);
    *dst.get_unchecked_mut(0) = (code >> 18 & 0x07).into_lossy() | TAG_FOUR_B;
    *dst.get_unchecked_mut(1) = (code >> 12 & 0x3F).into_lossy() | TAG_CONT;
    *dst.get_unchecked_mut(2) = (code >> 6 & 0x3F).into_lossy() | TAG_CONT;
    *dst.get_unchecked_mut(3) = (code & 0x3F).into_lossy() | TAG_CONT;
  }
}

/// Copies part of slice to another part (`mem::copy`, basically `memmove`)
#[inline]
unsafe fn shift_unchecked(s: &mut [u8], from: usize, to: usize, len: usize) {
  debug_assert!(to.saturating_add(len) <= s.len() && from.saturating_add(len) <= s.len());
  let (f, t) = (s.as_ptr().add(from), s.as_mut_ptr().add(to));
  copy(f, t, len);
}

/// Shifts string right
///
/// # Safety
///
/// It's UB if `to + (s.len() - from)` is bigger than [`S::to_usize()`]
///
/// [`<S as Unsigned>::to_usize()`]: ../struct.StaticString.html#CAPACITY
#[inline]
pub(crate) unsafe fn shift_right_unchecked<const N: usize>(
  s: &mut StaticString<N>,
  from: usize,
  to: usize,
)
{
  let len = s.len().saturating_sub(from);
  debug_assert!(from <= to && to.saturating_add(len) <= s.capacity());
  debug_assert!(s.as_str().is_char_boundary(from));
  shift_unchecked(s.as_mut_bytes(), from, to, len);
}

/// Shifts string left
#[inline]
pub(crate) unsafe fn shift_left_unchecked<const N: usize>(
  s: &mut StaticString<N>,
  from: usize,
  to: usize,
)
{
  debug_assert!(to <= from && from <= s.len());
  debug_assert!(s.as_str().is_char_boundary(from));

  let len = s.len().saturating_sub(to);
  shift_unchecked(s.as_mut_bytes(), from, to, len);
}

/// Returns error if size is outside of specified boundary
#[inline]
pub fn is_inside_boundary(size: usize, limit: usize) -> Result<(), Error> {
  Some(()).filter(|_| size <= limit).ok_or(Error::OutOfBounds)
}

/// Returns error if index is not at a valid utf-8 char boundary
#[inline]
pub fn is_char_boundary<const N: usize>(s: &StaticString<N>, idx: usize) -> Result<(), Error> {
  if s.as_str().is_char_boundary(idx) {
    return Ok(());
  }
  Err(Error::Utf8)
}

/// Truncates string to specified size (ignoring last bytes if they form a partial `char`)
#[inline]
pub(crate) fn truncate_str(slice: &str, size: usize) -> &str {
  if slice.is_char_boundary(size) {
    unsafe { slice.get_unchecked(..size) }
  } else if size < slice.len() {
    let mut index = size.saturating_sub(1);
    while !slice.is_char_boundary(index) {
      index = index.saturating_sub(1);
    }
    unsafe { slice.get_unchecked(..index) }
  } else {
    slice
  }
}

impl IntoLossy<u8> for usize {
  #[allow(clippy::cast_possible_truncation)]
  #[inline]
  fn into_lossy(self) -> u8 {
    self as u8
  }
}

impl IntoLossy<u8> for u32 {
  #[allow(clippy::cast_possible_truncation)]
  #[inline]
  fn into_lossy(self) -> u8 {
    self as u8
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use core::str::from_utf8;

  #[test]
  fn truncate() {
    assert_eq!(truncate_str("i", 10), "i");
    assert_eq!(truncate_str("iiiiii", 3), "iii");
    assert_eq!(truncate_str("🤔🤔🤔", 5), "🤔");
  }

  #[test]
  fn shift_right() {
    let _ = env_logger::try_init();
    let mut ls = SmallString::try_from_str("abcdefg").unwrap();
    unsafe { shift_right_unchecked(&mut ls, 0usize, 4usize) };
    ls.size += 4;
    assert_eq!(ls.as_str(), "abcdabcdefg");
  }

  #[test]
  fn shift_left() {
    let _ = env_logger::try_init();
    let mut ls = SmallString::try_from_str("abcdefg").unwrap();
    unsafe { shift_left_unchecked(&mut ls, 1usize, 0usize) };
    ls.size -= 1;
    assert_eq!(ls.as_str(), "bcdefg");
  }

  #[test]
  fn shift_nop() {
    let _ = env_logger::try_init();
    let mut ls = SmallString::try_from_str("abcdefg").unwrap();
    unsafe { shift_right_unchecked(&mut ls, 0usize, 0usize) };
    assert_eq!(ls.as_str(), "abcdefg");
    unsafe { shift_left_unchecked(&mut ls, 0usize, 0usize) };
    assert_eq!(ls.as_str(), "abcdefg");
  }

  #[test]
  fn encode_char_utf8() {
    let _ = env_logger::try_init();
    let mut string = SmallString::default();
    unsafe {
      encode_char_utf8_unchecked(&mut string, 'a', 0);
      assert_eq!(from_utf8(&string.as_mut_bytes()[..1]).unwrap(), "a");
      let mut string = SmallString::try_from_str("a").unwrap();

      encode_char_utf8_unchecked(&mut string, '🤔', 1);
      assert_eq!(from_utf8(&string.as_mut_bytes()[..5]).unwrap(), "a🤔");
    }
  }
}
