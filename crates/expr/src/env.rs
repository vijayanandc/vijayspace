//! The shared, immutable template environment (filters registered once).

use std::sync::LazyLock;

use minijinja::{Environment, UndefinedBehavior};

/// Instruction budget per render: protects workers from runaway loops in templates.
const FUEL: u64 = 50_000;

static ENV: LazyLock<Environment<'static>> = LazyLock::new(build);

pub(crate) fn env() -> &'static Environment<'static> {
    &ENV
}

fn build() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Lenient);
    env.set_fuel(Some(FUEL));
    env.set_recursion_limit(64);
    crate::filters::register(&mut env);
    env
}
