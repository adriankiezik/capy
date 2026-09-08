use std::collections::HashSet;
use winit::keyboard::PhysicalKey;

#[derive(Debug, Default)]
pub struct Input {
    down: HashSet<PhysicalKey>,
    pressed: HashSet<PhysicalKey>,
    released: HashSet<PhysicalKey>,
}

impl Input {
    pub fn down(&self, key: impl Into<PhysicalKey>) -> bool {
        self.down.contains(&key.into())
    }

    pub fn pressed(&self, key: impl Into<PhysicalKey>) -> bool {
        self.pressed.contains(&key.into())
    }

    pub fn released(&self, key: impl Into<PhysicalKey>) -> bool {
        self.released.contains(&key.into())
    }

    pub(super) fn change(&mut self, key: PhysicalKey, down: bool) {
        if down {
            if self.down.insert(key) {
                self.pressed.insert(key);
            }
        } else if self.down.remove(&key) {
            self.released.insert(key);
        }
    }

    pub(super) fn release_all(&mut self) {
        self.released.extend(self.down.drain());

        self.pressed.clear();
    }

    pub(super) fn finish_update(&mut self) {
        self.pressed.clear();

        self.released.clear();
    }
}
