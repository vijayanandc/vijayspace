# weft-script
Custom functions in Rhai (doc 07 §9, doc 08 §7). **Pure: no IO, no clock, no async.**
- `check(code)` compiles only (publish-time validation, with line and column).
- `run(code, input, context, limits)` runs one script in a fresh engine with every limit set.
- Helpers live in `helpers.rs` and call `weft_expr::funcs`, so templates and scripts agree.
- Never enable rhai's `unchecked` feature; never register functions with IO.
- Every helper and every hostile-script case has a test in `tests.rs`.
