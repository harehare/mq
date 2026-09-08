# Fuzzing mq

Run the tree-walking evaluator and Tarn VM independently:

```sh
just test-fuzz
just test-fuzz-tarn
```

To compare both engines for the same deterministic programs, run:

```sh
just test-fuzz-differential 1000
```

The differential target builds a tree-walker helper and a Tarn helper separately, then compares
their successful values (or whether both reject a program). Its intentionally small language
subset covers expressions, conditionals, closures, `foreach`, and destructuring without treating
implementation-specific features or nondeterministic builtins as mismatches.

It also generates compound boolean predicates, nested conditions, guarded matches, and multi-step
`filter`/`map` pipelines so agreement is checked beyond single-condition queries.

The helpers are cached under `fuzz/target/`; set `MQ_DIFF_TREE_TARGET_DIR` or
`MQ_DIFF_VM_TARGET_DIR` to use different build directories.
