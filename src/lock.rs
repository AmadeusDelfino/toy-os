use core::ops::{Deref, DerefMut};

/// A wrapper around spin::Mutex to permit trait implementations.
pub struct Locked<A> {
    inner: spin::Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: spin::Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<'_, A> {
        self.inner.lock()
    }
}

pub struct LockedIrq<A> {
    inner: spin::Mutex<A>,
}

pub struct LockedIrqGuard<'a, A> {
    saved_iflag: bool,
    guard: Option<spin::MutexGuard<'a, A>>,
}

impl<A> LockedIrq<A> {
    pub const fn new(inner: A) -> Self {
        Self {
            inner: spin::Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> LockedIrqGuard<'_, A> {
        use x86_64::instructions::interrupts;

        let saved_iflag = interrupts::are_enabled();
        interrupts::disable();
        let guard = self.inner.lock();

        LockedIrqGuard {
            saved_iflag,
            guard: Some(guard),
        }
    }
}

impl<A> Deref for LockedIrqGuard<'_, A> {
    type Target = A;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap().deref()
    }
}

impl<A> DerefMut for LockedIrqGuard<'_, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().unwrap().deref_mut()
    }
}

impl<A> Drop for LockedIrqGuard<'_, A> {
    fn drop(&mut self) {
        use x86_64::instructions::interrupts;

        drop(self.guard.take());

        if self.saved_iflag {
            interrupts::enable();
        }
    }
}
