use super::{Asset, manager::Entry};
use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::{Arc, Weak},
};

pub struct Handle<T: Asset> {
    pub(super) value: Arc<T>,
    pub(super) entry: Arc<Entry<T>>,
}

impl<T: Asset> Handle<T> {
    pub(crate) fn downgrade(&self) -> Weak<T> {
        Arc::downgrade(&self.value)
    }
}

impl<T: Asset> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            entry: self.entry.clone(),
        }
    }
}

impl<T: Asset> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handle")
            .field("type", &std::any::type_name::<T>())
            .field("value", &Arc::as_ptr(&self.value))
            .finish()
    }
}

impl<T: Asset> Deref for Handle<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: Asset> AsRef<T> for Handle<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T: Asset> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }
}

impl<T: Asset> Eq for Handle<T> {}

impl<T: Asset> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.value).hash(state);
    }
}
