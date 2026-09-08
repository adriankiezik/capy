use super::{Axis2Binding, AxisBinding, Button, ButtonBinding, ButtonState, Input};
use glam::Vec2;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    time::Duration,
};
use winit::{
    event::MouseButton,
    keyboard::{KeyCode, PhysicalKey},
};

#[derive(Debug, Clone)]
pub enum Binding {
    Button(ButtonBinding),
    Axis(AxisBinding),
    Axis2(Axis2Binding),
}

impl From<ButtonBinding> for Binding {
    fn from(binding: ButtonBinding) -> Self {
        Self::Button(binding)
    }
}

impl From<Button> for Binding {
    fn from(button: Button) -> Self {
        ButtonBinding::new(button).into()
    }
}

impl From<KeyCode> for Binding {
    fn from(key: KeyCode) -> Self {
        ButtonBinding::new(key).into()
    }
}

impl From<PhysicalKey> for Binding {
    fn from(key: PhysicalKey) -> Self {
        ButtonBinding::new(key).into()
    }
}

impl From<MouseButton> for Binding {
    fn from(button: MouseButton) -> Self {
        ButtonBinding::new(button).into()
    }
}

impl From<AxisBinding> for Binding {
    fn from(binding: AxisBinding) -> Self {
        Self::Axis(binding)
    }
}

impl From<Axis2Binding> for Binding {
    fn from(binding: Axis2Binding) -> Self {
        Self::Axis2(binding)
    }
}

#[derive(Debug, Clone)]
enum ActionBinding {
    Button(ButtonBinding),
    Axis(Vec<AxisBinding>),
    Axis2(Vec<Axis2Binding>),
}

impl ActionBinding {
    fn new(binding: Binding) -> Self {
        match binding {
            Binding::Button(binding) => Self::Button(binding),
            Binding::Axis(binding) => Self::Axis(vec![binding]),
            Binding::Axis2(binding) => Self::Axis2(vec![binding]),
        }
    }

    fn add(&mut self, binding: Binding) {
        match (self, binding) {
            (Self::Button(existing), Binding::Button(binding)) => {
                for button in binding.buttons {
                    if !existing.buttons.contains(&button) {
                        existing.buttons.push(button);
                    }
                }
            }
            (Self::Axis(existing), Binding::Axis(binding)) => existing.push(binding),
            (Self::Axis2(existing), Binding::Axis2(binding)) => existing.push(binding),
            (existing, binding) => *existing = Self::new(binding),
        }
    }

    fn buttons(&self) -> Vec<Button> {
        match self {
            Self::Button(binding) => binding.buttons.clone(),
            Self::Axis(bindings) => bindings.iter().flat_map(AxisBinding::buttons).collect(),
            Self::Axis2(bindings) => bindings.iter().flat_map(Axis2Binding::buttons).collect(),
        }
    }

    fn consumed_buttons(&self, input: &Input) -> Vec<Button> {
        match self {
            Self::Axis2(bindings) => bindings
                .iter()
                .filter(|binding| binding.enabled(input))
                .flat_map(Axis2Binding::buttons)
                .collect(),
            _ => self.buttons(),
        }
    }

    fn uses_mouse_motion(&self, input: &Input) -> bool {
        matches!(self, Self::Axis2(bindings) if bindings.iter().any(|binding| binding.enabled(input) && binding.uses_mouse_motion()))
    }

    fn value(&self, input: &Input, delta: Duration) -> ActionValue {
        match self {
            Self::Button(binding) => ActionValue::Button(binding.state(input)),
            Self::Axis(bindings) => {
                ActionValue::Axis(bindings.iter().map(|binding| binding.value(input)).sum())
            }
            Self::Axis2(bindings) => ActionValue::Axis2(
                bindings
                    .iter()
                    .map(|binding| binding.value(input, delta))
                    .sum(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ActionValue {
    Button(ButtonState),
    Axis(f32),
    Axis2(Vec2),
}

#[derive(Debug)]
pub struct Actions<A> {
    values: HashMap<A, ActionValue>,
}

impl<A> Default for Actions<A> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

impl<A: Eq + Hash> Actions<A> {
    pub fn button(&self, action: A) -> ButtonState {
        match self.values.get(&action) {
            Some(ActionValue::Button(state)) => *state,
            _ => ButtonState::default(),
        }
    }

    pub fn pressed(&self, action: A) -> bool {
        self.button(action).pressed()
    }

    pub fn just_pressed(&self, action: A) -> bool {
        self.button(action).just_pressed()
    }

    pub fn just_released(&self, action: A) -> bool {
        self.button(action).just_released()
    }

    pub fn active(&self, action: A) -> bool {
        self.button(action).active()
    }

    pub fn axis(&self, action: A) -> f32 {
        match self.values.get(&action) {
            Some(ActionValue::Axis(value)) => *value,
            _ => 0.0,
        }
    }

    pub fn axis2(&self, action: A) -> Vec2 {
        match self.values.get(&action) {
            Some(ActionValue::Axis2(value)) => *value,
            _ => Vec2::ZERO,
        }
    }
}

#[derive(Debug)]
pub struct InputContext<A> {
    bindings: Vec<(A, ActionBinding)>,
    priority: i32,
    active: bool,
    was_active: bool,
    consume_input: bool,
    require_reset: bool,
    blocked: HashSet<Button>,
    claimed: HashSet<Button>,
}

impl<A> Default for InputContext<A> {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            priority: 0,
            active: true,
            was_active: false,
            consume_input: true,
            require_reset: true,
            blocked: HashSet::new(),
            claimed: HashSet::new(),
        }
    }
}

impl<A: Eq> InputContext<A> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(mut self, action: A, binding: impl Into<Binding>) -> Self {
        self.insert(action, binding);

        self
    }

    pub fn insert(&mut self, action: A, binding: impl Into<Binding>) {
        let binding = binding.into();

        if let Some((_, existing)) = self.bindings.iter_mut().find(|(key, _)| *key == action) {
            existing.add(binding);
        } else {
            self.bindings.push((action, ActionBinding::new(binding)));
        }

        self.was_active = false;
    }

    pub fn remove(&mut self, action: A) {
        self.bindings.retain(|(key, _)| *key != action);

        self.was_active = false;
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;

        self
    }

    pub fn with_consumption(mut self, consume: bool) -> Self {
        self.consume_input = consume;

        self
    }

    pub fn with_require_reset(mut self, require_reset: bool) -> Self {
        self.require_reset = require_reset;

        self
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;

        self
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn active(&self) -> bool {
        self.active
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputContextId(usize);

#[derive(Debug)]
pub struct InputMap<A> {
    contexts: Vec<InputContext<A>>,
    retired: HashSet<Button>,
    actions: Actions<A>,
}

impl<A> Default for InputMap<A> {
    fn default() -> Self {
        Self {
            contexts: vec![InputContext::default()],
            retired: HashSet::new(),
            actions: Actions::default(),
        }
    }
}

impl<A: Copy + Eq + Hash> InputMap<A> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(mut self, action: A, binding: impl Into<Binding>) -> Self {
        self.insert(action, binding);

        self
    }

    pub fn insert(&mut self, action: A, binding: impl Into<Binding>) {
        self.contexts[0].insert(action, binding);
    }

    pub fn remove(&mut self, action: A) {
        self.contexts[0].remove(action);
    }

    pub fn add_context(&mut self, context: InputContext<A>) -> InputContextId {
        let id = InputContextId(self.contexts.len());

        self.contexts.push(context);

        id
    }

    pub fn context_mut(&mut self, id: InputContextId) -> Option<&mut InputContext<A>> {
        self.contexts.get_mut(id.0)
    }

    pub fn update(&mut self, input: &Input, delta: Duration) -> &Actions<A> {
        let mut routed = input.clone();

        for context in &mut self.contexts {
            if !context.active || !context.was_active {
                self.retired.extend(context.claimed.drain());
            }

            if context.active && !context.was_active && context.require_reset {
                context.blocked.extend(
                    context
                        .bindings
                        .iter()
                        .flat_map(|(_, binding)| binding.buttons())
                        .filter(|button| input.initial_down.contains(button)),
                );
            }

            context.was_active = context.active;
        }

        self.retired.retain(|&button| routed.suppress_held(button));

        let mut order: Vec<_> = (0..self.contexts.len()).collect();

        order.sort_by_key(|&index| std::cmp::Reverse(self.contexts[index].priority));

        let mut values = HashMap::new();

        for index in order {
            let context = &mut self.contexts[index];

            if !context.active {
                context.blocked.clear();

                continue;
            }

            let mut local = routed.clone();

            context.blocked.retain(|&button| {
                let blocked = local.suppress_held(button);

                if context.consume_input {
                    routed.suppress_held(button);
                }

                blocked
            });

            context.claimed.clear();

            for (action, binding) in &context.bindings {
                values
                    .entry(*action)
                    .or_insert_with(|| binding.value(&local, delta));

                if context.consume_input {
                    for button in binding.consumed_buttons(&local) {
                        routed.hide(button);

                        if local.button(button).pressed() || context.blocked.contains(&button) {
                            context.claimed.insert(button);
                        }
                    }

                    if binding.uses_mouse_motion(&local) {
                        routed.discard_mouse_motion();
                    }
                }
            }
        }

        for (&action, &previous) in &self.actions.values {
            if let ActionValue::Button(previous) = previous
                && previous.pressed()
            {
                let value = values
                    .entry(action)
                    .or_insert(ActionValue::Button(ButtonState::default()));

                if let ActionValue::Button(state) = value {
                    state.just_released |= !state.pressed;
                }
            }
        }

        for (&action, value) in &mut values {
            if let ActionValue::Button(state) = value {
                state.just_pressed |= state.pressed && !self.actions.pressed(action);
            }
        }

        self.actions.values = values;

        &self.actions
    }
}
