pub(crate) struct SyncWrapper<T>(T);

impl<T> SyncWrapper<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self(value)
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

unsafe impl<T> Sync for SyncWrapper<T> {}
