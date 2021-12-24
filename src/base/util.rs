use parking_lot::{Condvar, Mutex, RwLockReadGuard, RwLockUpgradableReadGuard};

pub(crate) struct CondvarAny {
    c: Condvar,
    m: Mutex<()>,
}
impl CondvarAny {
    pub(crate) fn new() -> Self {
        Self {
            c: Condvar::new(),
            m: Mutex::new(()),
        }
    }

    pub(crate) fn notify_all(&self)->usize {
        self.c.notify_all()
    }

    pub(crate) fn wait<T>(&self, g: &mut RwLockReadGuard<'_, T>) {
        let guard = self.m.lock();
        RwLockReadGuard::unlocked(g, || {
            // Move the guard in so it gets unlocked before we re-lock g
            let mut guard = guard;
            self.c.wait(&mut guard);
        });
    }

    pub(crate) fn wait2<T>(&self, g: &mut RwLockUpgradableReadGuard<'_, T>) {
        let guard = self.m.lock();
        RwLockUpgradableReadGuard::unlocked(g, || {
            // Move the guard in so it gets unlocked before we re-lock g
            let mut guard = guard;
            self.c.wait(&mut guard);
        });
    }

}
