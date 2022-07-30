pub(crate) fn parallel_try_fill<I, T, E>(iter: I, slice: &mut [T]) -> Result<(), E>
where
    I: IndexedParallelIterator<Item = Result<T, E>>,
    T: Send,
    E: Send,
{
    assert_eq!(iter.len(), slice.len());

    iter.drive(Consumer {
        slice,
        error: PhantomData,
    })
}

struct Consumer<'slice, T, E> {
    slice: &'slice mut [T],
    error: PhantomData<fn() -> E>,
}

impl<'slice, T, E> rayon::iter::plumbing::Consumer<Result<T, E>> for Consumer<'slice, T, E>
where
    T: Send,
    E: Send,
{
    type Folder = Folder<'slice, T, E>;
    type Reducer = Reducer;
    type Result = Result<(), E>;

    fn split_at(self, index: usize) -> (Self, Self, Self::Reducer) {
        let (a, b) = self.slice.split_at_mut(index);
        let a = Self {
            slice: a,
            error: PhantomData,
        };
        let b = Self {
            slice: b,
            error: PhantomData,
        };
        (a, b, Reducer)
    }
    fn into_folder(self) -> Self::Folder {
        Folder(Ok(self.slice))
    }
    fn full(&self) -> bool {
        self.slice.is_empty()
    }
}

struct Folder<'slice, T, E>(Result<&'slice mut [T], E>);

impl<'slice, T, E> rayon::iter::plumbing::Folder<Result<T, E>> for Folder<'slice, T, E> {
    type Result = Result<(), E>;

    fn consume(self, item: Result<T, E>) -> Self {
        match (self.0, item) {
            (Ok([reference, rest @ ..]), Ok(item)) => {
                *reference = item;
                Self(Ok(rest))
            }
            (Err(error), _) | (_, Err(error)) => Self(Err(error)),
            (Ok([]), _) => panic!(),
        }
    }
    fn complete(self) -> Self::Result {
        self.0.map(drop)
    }
    fn full(&self) -> bool {
        self.0.is_err()
    }
}

struct Reducer;

impl<E> rayon::iter::plumbing::Reducer<Result<(), E>> for Reducer {
    fn reduce(self, left: Result<(), E>, right: Result<(), E>) -> Result<(), E> {
        left.and(right)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ok() {
        let mut res = vec![0; 1024];
        parallel_try_fill((0..1024).into_par_iter().map(Ok::<usize, ()>), &mut res).unwrap();
        for (i, v) in res.into_iter().enumerate() {
            assert_eq!(i, v);
        }
    }

    #[test]
    fn one_error() {
        let mut buf = vec![0; 1025];
        let res = parallel_try_fill((0..1024).into_par_iter().map(Ok).chain([Err(())]), &mut buf);
        assert_eq!(res, Err(()));
    }

    use super::parallel_try_fill;
    use rayon::iter::IntoParallelIterator;
    use rayon::iter::ParallelIterator;
}

use rayon::iter::IndexedParallelIterator;
use std::marker::PhantomData;
