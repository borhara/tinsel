use std::io;
use std::time::{Duration, Instant};

use super::{Event, Key, input};
use crate::term;

/// Keys read one at a time in the normal terminal, outside a full-screen [`run`](super::run):
/// for a loop that redraws now and then and reacts to a key in between.
///
/// While it's open, what's typed isn't shown and Ctrl-C arrives as `Key::Ctrl('c')` instead
/// of stopping the program. Dropping it puts the terminal back. While a
/// [`prompt::script`](crate::prompt::script) runs on this thread, keys come from the script
/// instead, and once it has none left nothing is pressed.
///
/// ```no_run
/// use std::time::Duration;
/// use tinsel::app::{Key, Keys};
///
/// let mut keys = Keys::open()?;
/// loop {
///     // show what changed
///     match keys.next(Duration::from_secs(4))? {
///         Some(Key::Char('q') | Key::Ctrl('c')) => break,
///         _ => {}
///     }
/// }
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct Keys {
    pending: Vec<u8>,
    raw: Option<term::RawMode>,
}

impl Keys {
    /// Start reading keys from the terminal on stdin. Fails without one.
    pub fn open() -> io::Result<Keys> {
        let raw = if crate::prompt::scripted() {
            None
        } else {
            Some(term::RawMode::enter()?)
        };
        Ok(Keys {
            pending: Vec::new(),
            raw,
        })
    }

    /// The next key, waiting up to `timeout` (to within a tenth of a second) for one, or
    /// `None` if nothing was pressed. Mouse reports and anything else that isn't a key are
    /// skipped.
    pub fn next(&mut self, timeout: Duration) -> io::Result<Option<Key>> {
        if self.raw.is_none() {
            return Ok(crate::prompt::next_scripted());
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(key) = self.take(false) {
                return Ok(Some(key));
            }
            match term::read_byte()? {
                Some(b) => self.pending.push(b),
                // a pause ends whatever was half read, such as a lone Esc
                None => {
                    if let Some(key) = self.take(true) {
                        return Ok(Some(key));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Ok(self.take(true));
            }
        }
    }

    fn take(&mut self, done: bool) -> Option<Key> {
        while let Some((event, used)) = input::parse(&self.pending, done) {
            self.pending.drain(..used);
            if let Some(Event::Key(key)) = event {
                return Some(key);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_come_from_a_script_and_then_stop() {
        let _script = crate::prompt::script([Key::Char('r'), Key::Ctrl('c')]);
        let mut keys = Keys::open().unwrap();
        let wait = Duration::from_secs(5);
        assert_eq!(keys.next(wait).unwrap(), Some(Key::Char('r')));
        assert_eq!(keys.next(wait).unwrap(), Some(Key::Ctrl('c')));
        assert_eq!(keys.next(wait).unwrap(), None);
    }

    #[test]
    fn half_read_bytes_become_keys() {
        let mut keys = Keys {
            pending: b"\x1b[Ax\x1b".to_vec(),
            raw: None,
        };
        assert_eq!(keys.take(false), Some(Key::Up));
        assert_eq!(keys.take(false), Some(Key::Char('x')));
        assert_eq!(keys.take(false), None);
        assert_eq!(keys.take(true), Some(Key::Esc));
    }
}
