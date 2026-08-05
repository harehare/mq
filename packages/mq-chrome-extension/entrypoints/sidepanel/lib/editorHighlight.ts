import { HighlightStyle } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

export const editorHighlightStyle = HighlightStyle.define([
  { tag: t.comment, color: "light-dark(#008000, #6A9955)", fontStyle: "italic" },
  { tag: t.keyword, color: "light-dark(#0000ff, #569cd6)", fontWeight: "bold" },
  { tag: t.function(t.variableName), color: "light-dark(#795e26, #dcdcaa)" },
  { tag: t.variableName, color: "light-dark(#001080, #9cdcfe)" },
  { tag: t.propertyName, color: "light-dark(#001080, #9cdcfe)" },
  { tag: t.string, color: "light-dark(#a31515, #ce9178)" },
  { tag: t.escape, color: "light-dark(#a31515, #ce9178)" },
  { tag: t.number, color: "light-dark(#098658, #b5cea8)" },
  {
    tag: [t.operator, t.punctuation, t.paren, t.bracket],
    color: "light-dark(#333333, #d4d4d4)",
  },
  { tag: t.heading, color: "light-dark(#2c5282, #67b8e3)", fontWeight: "bold" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  {
    tag: [t.link, t.url],
    color: "light-dark(#3182ce, #63b3ed)",
    textDecoration: "underline",
  },
  { tag: t.tagName, color: "light-dark(#0000ff, #569cd6)" },
  { tag: t.attributeName, color: "light-dark(#795e26, #9cdcfe)" },
  { tag: t.attributeValue, color: "light-dark(#a31515, #ce9178)" },
  { tag: t.meta, color: "light-dark(#718096, #a0aec0)" },
  { tag: t.invalid, color: "#e53e3e" },
]);
