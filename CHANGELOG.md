# Changelog

## Unreleased

- Remove the `mq-lang` `IncrementalParser` and `TextEdit` APIs. Use `parse_recovery` or `CstParser` to parse the complete source.
- Correct CST node ranges for expressions whose first token belongs to a child node, including binary expressions.
- Return the definition name from `CstNode::get_identifier` for `def` nodes.
- Report lexer failures through `parse_recovery` instead of panicking.
