#![warn(
    clippy::pedantic,
    noop_method_call,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_op_in_unsafe_fn,
    unused_lifetimes,
    unused_qualifications
)]
#![allow(clippy::missing_errors_doc)]

pub mod fs;

pub use history::History;
pub mod history {
    #[derive(Debug, Clone, bincode::Encode)]
    pub struct History<T> {
        stack: Vec<T>,
        current: usize,
    }

    impl<T: PartialEq> History<T> {
        pub fn new(initial: T) -> Self {
            Self {
                stack: vec![initial],
                current: 0,
            }
        }
        #[must_use]
        pub fn current(&self) -> &T {
            &self.stack[self.current]
        }
        pub fn push(&mut self, state: T) {
            self.stack.truncate(self.current + 1);
            if *self.current() != state {
                self.stack.push(state);
                self.current += 1;
            }
        }
        pub fn back(&mut self) {
            self.current = self.current.saturating_sub(1);
        }
        pub fn forward(&mut self) {
            self.current = (self.current + 1).min(self.stack.len() - 1);
        }
        pub fn map<U, F: FnMut(T) -> U>(self, f: F) -> History<U> {
            History {
                stack: self.stack.into_iter().map(f).collect(),
                current: self.current,
            }
        }
        pub fn ref_map<U, F: FnMut(&T) -> U>(&self, f: F) -> History<U> {
            History {
                stack: self.stack.iter().map(f).collect(),
                current: self.current,
            }
        }
    }

    impl<T: PartialEq + Default> Default for History<T> {
        fn default() -> Self {
            Self::new(T::default())
        }
    }

    impl<T: bincode::Decode> bincode::Decode for History<T> {
        fn decode<D: bincode::de::Decoder>(
            decoder: &mut D,
        ) -> Result<Self, bincode::error::DecodeError> {
            let stack = <Vec<T>>::decode(decoder)?;
            let current = usize::decode(decoder)?;
            if stack.len() <= current {
                return Err(bincode::error::DecodeError::OtherString(format!(
                    "history index `{current}` is out of bounds"
                )));
            }
            Ok(Self { stack, current })
        }
    }
}
