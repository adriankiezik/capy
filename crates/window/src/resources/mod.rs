use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// Generates a callback resource that stores `fn` pointers and invokes them sequentially.
macro_rules! callback_resource {
    ($name:ident ( $($arg:ident : $ty:ty),* $(,)? )) => {
        #[derive(Resource, Default)]
        pub struct $name {
            callbacks: Vec<fn($($ty),*)>,
        }

        impl $name {
            pub fn add(&mut self, callback: fn($($ty),*)) {
                self.callbacks.push(callback);
            }

            pub(crate) fn invoke(&self, $($arg: $ty),*) {
                for callback in &self.callbacks {
                    callback($($arg),*);
                }
            }
        }
    };
}

/// Generates a callback resource with bool-returning `fn` pointers and a custom aggregator.
macro_rules! callback_resource_bool {
    ($name:ident ( $($arg:ident : $ty:ty),* $(,)? ), $aggregator:expr) => {
        #[derive(Resource, Default)]
        pub struct $name {
            callbacks: Vec<fn($($ty),*) -> bool>,
        }

        impl $name {
            pub fn add(&mut self, callback: fn($($ty),*) -> bool) {
                self.callbacks.push(callback);
            }

            pub(crate) fn invoke(&self, $($arg: $ty),*) -> bool {
                let aggregator: fn(&[fn($($ty),*) -> bool], $($ty),*) -> bool = $aggregator;
                aggregator(&self.callbacks, $($arg),*)
            }
        }
    };
}

callback_resource!(OnAppResumed(world: &mut World, event_loop: &ActiveEventLoop));
callback_resource!(OnBeginFrame(world: &mut World, window: &Window));
callback_resource!(OnEndFrame(world: &mut World, window: &Window));

callback_resource_bool!(OnWindowEvent(world: &mut World, window: &Window, event: &WindowEvent),
    |callbacks, world, window, event| {
        let mut consumed = false;
        for callback in callbacks {
            consumed |= callback(world, window, event);
        }
        consumed
    }
);

callback_resource_bool!(WantsPointerInput(world: &World),
    |callbacks, world| callbacks.iter().any(|callback| callback(world))
);
