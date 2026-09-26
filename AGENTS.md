# qa-intent

Working directory: this repository root. Rust library entry is src/lib.rs; CLI entry is src/main.rs.
Compiler maps QaItem + Taxonomy to System One envelopes. Expected labels never enter inference state.
qid identifies a question; it is not a model head. choice.confidence is not answer_confidence.
Do not add generated artifacts, feedback data, credentials or local configuration to Git.

Validation: cargo fmt --check; cargo clippy --all-targets --locked -- -D warnings; cargo test --locked.
Packaging: build native target, run npm/scripts/gen-platform-packages.mjs <target>,
then npm run test:package. Keep Cargo and all npm package versions aligned.
Five target definitions live only in npm/bin/targets.cjs. Missing binaries must fail packaging.
Only ci.yml publishes to npm. aio-cli.yml only syncs the matching release to AIO,
whose identity contract requires that exact workflow filename.
Authentication failure must remain a failure; no false green release.

generator turns natural-language scenarios into bank::QuestionBank using a text model.
bank validates/exports FAQ data; answers and generated labels never enter classification state.
Only human-reviewed bank items become QaItem.expected. Generated labels remain marked synthetic.
README.md is the only npm/GitHub usage source; packaging must preserve it byte-for-byte.
